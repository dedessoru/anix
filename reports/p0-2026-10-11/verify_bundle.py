import json
import re
from collect import ROOT, OUT, digest, manifest, save


def main():
    before = json.loads((OUT / 'source-before.json').read_text(encoding='utf-8'))
    after = manifest()
    matrix = json.loads((OUT / 'gap-matrix.json').read_text(encoding='utf-8'))
    symbols = json.loads((OUT / 'symbols.json').read_text(encoding='utf-8'))
    known = {s['symbol'] for s in symbols}
    missing = []
    for row in matrix:
        for name in row['existing_coverage'].split(';'):
            name = name.strip().split(' ')[0]
            if re.fullmatch(r'[a-z][a-z0-9_]+', name) and '_' in name and name not in known:
                missing.append({'contract': row['id'], 'symbol': name})
    assert not missing, missing
    assert [r['id'] for r in matrix] == [f'I{i:02}' for i in range(1, 27)] + [f'M{i}' for i in range(18, 25)]
    assert before == after, 'Application source changed'
    files = [p for p in OUT.iterdir() if p.is_file() and p.suffix not in ('.exe', '.sqlite') and '.local.' not in p.name and p.name not in ('bundle-manifest.json', 'bundle-verification.json')]
    save('bundle-manifest.json', {p.name: {'bytes': p.stat().st_size, 'sha256': digest(p)} for p in sorted(files)})
    save('bundle-verification.json', {'source_unchanged': before == after, 'contract_rows': len(matrix), 'coverage_symbols_exist': True, 'status': 'P0_INCOMPLETE for full stage1 scope; existing baseline checks PASS', 'limits': 'Validation of audit consistency, not independent contract/science verification'})
    print('verified: source unchanged; 33 contracts; named coverage symbols exist')


if __name__ == '__main__':
    main()
