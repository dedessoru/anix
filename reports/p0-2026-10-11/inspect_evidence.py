"""Inventory historical evidence and derive diagnostic indices, without replay I/O."""
import json
import re
import sqlite3
from pathlib import Path
from collect import ROOT, OUT, digest, save


def main():
    files = []
    for directory in ('crates', 'tests', 'src', 'migrations', 'config', 'models/profiles'):
        files.extend(p for p in (ROOT / directory).rglob('*') if p.is_file())
    pattern = re.compile(r'\b(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:fn|struct|enum|trait)\s+(\w+)')
    symbols = []
    for p in files:
        if p.suffix != '.rs':
            continue
        for line, text in enumerate(p.read_text(encoding='utf-8').splitlines(), 1):
            for m in pattern.finditer(text):
                symbols.append({'path': p.relative_to(ROOT).as_posix(), 'line': line, 'symbol': m[1]})
    save('symbols.json', symbols)
    old = json.loads((ROOT / '.runtime/anix-v1-session13-manifest.json').read_text(encoding='utf-8-sig'))
    comparisons = []
    for row in old:
        name = row['Path'].replace('\\', '/').split('researcher-s-notes-core/')[-1]
        p = ROOT / name
        actual = digest(p) if p.is_file() else None
        comparisons.append({'path': name, 'historical_sha256': row['Hash'].lower(), 'current_sha256': actual, 'matches': actual == row['Hash'].lower()})
    save('historical-checkpoint-comparison.json', comparisons)
    runs = [('8b-654', 'long-context', 654, '20261010T081048725'), ('14b-654', 'long-context', 654, '20261010T081319661'), ('14b-664', 'branching', 664, '20261010T081513351'), ('8b-664-iteration12', 'branching', 664, '20261010T075935935')]
    inventory, diagnostic = [], []
    for label, scenario, seed, stamp in runs:
        folder = ROOT / f'.runtime/lead-runtime-cleanup-seeds/{scenario}-{seed}/eval-results/long-horizon/runs/{stamp}/{scenario}-{seed:06}'
        for name in ('report.json', 'lead-decision-trace.jsonl'):
            p = folder / name
            inventory.append({'label': label, 'path': p.relative_to(ROOT).as_posix(), 'exists': p.is_file(), 'sha256': digest(p) if p.is_file() else None})
        trace = folder / 'lead-decision-trace.jsonl'
        if trace.exists():
            rows = [json.loads(s) for s in trace.read_text(encoding='utf-8-sig').splitlines()]
            diagnostic.append({'label': label, 'row_count': len(rows), 'top_level_keys': list(rows[0]) if rows else [], 'trace': trace.relative_to(ROOT).as_posix()})
            # Full originals remain local: never publish unreviewed provider payloads.
    save('historical-run-inventory.json', inventory)
    save('trace-index.json', diagnostic)
    # SQLite backup API reads the original in read-only mode, capturing WAL too.
    db_path = ROOT / '.anix/state.sqlite'
    if db_path.exists():
        copy = OUT / 'isolated/state.sqlite'
        copy.parent.mkdir(exist_ok=True)
        with sqlite3.connect(db_path.as_uri() + '?mode=ro', uri=True) as src, sqlite3.connect(copy) as dest:
            src.backup(dest)
        with sqlite3.connect(copy) as db:
            tables = [r[0] for r in db.execute("select name from sqlite_master where type='table' order by name")]
            save('isolated-db-inventory.json', {'original': str(db_path), 'original_sha256': digest(db_path), 'copy': str(copy), 'copy_sha256': digest(copy), 'integrity_check': db.execute('pragma integrity_check').fetchall(), 'migrations': db.execute('select version,name,checksum from schema_migrations order by version').fetchall(), 'row_counts': {t: db.execute('select count(*) from "' + t.replace('"', '""') + '"').fetchone()[0] for t in tables}})
    print(json.dumps(diagnostic, indent=2))
    print('checkpoint:', json.dumps(comparisons, indent=2))


if __name__ == '__main__':
    main()
