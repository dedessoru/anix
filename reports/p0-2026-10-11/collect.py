"""Read-only P0 evidence collector. Never changes application inputs or user DBs."""
import hashlib
import json
import os
import platform
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def save(name, value):
    (OUT / name).write_text(json.dumps(value, indent=2, ensure_ascii=False), encoding='utf-8')


def manifest():
    paths = [ROOT / name for name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'README.md', 'todo.md', '.gitignore', 'anix-stage1-integrated-architecture-plan-v1.1.md')]
    for name in ('crates', 'src', 'tests', 'migrations', 'config', 'scripts', 'models'):
        paths.extend(p for p in (ROOT / name).rglob('*') if p.is_file() and p.suffix not in ('.gguf', '.bin', '.safetensors', '.partial'))
    return {p.relative_to(ROOT).as_posix(): {'bytes': p.stat().st_size, 'sha256': digest(p)} for p in sorted(set(paths)) if p.exists()}


def run_checks():
    save('source-before.json', manifest())
    rustc = subprocess.check_output(['rustup', 'which', '--toolchain', 'stable', 'rustc'], cwd=ROOT, text=True).strip()
    bindir = Path(rustc).parent
    env = dict(os.environ, RUSTC=rustc, RUSTDOC=str(bindir / 'rustdoc.exe'), RUSTUP_TOOLCHAIN='stable')
    env['PATH'] = str(bindir) + os.pathsep + env['PATH']
    cargo = str(bindir / 'cargo.exe')
    def output(cmd):
        return subprocess.check_output(cmd, cwd=ROOT, env=env, text=True).strip()
    save('environment.json', {'workspace': str(ROOT), 'revision': output(['git', 'rev-parse', 'HEAD']), 'dirty': output(['git', 'status', '--porcelain']), 'os': platform.platform(), 'python': sys.version, 'rustc': output([rustc, '-Vv']), 'cargo': output([cargo, '-V']), 'rustc_path': rustc, 'rustup_toolchain': 'stable', 'target_dir': 'target/p0-audit', 'sampling_seed': 'NOT RUN', 'network_model_calls': 'NONE'})
    commands = [
        ('fmt', [cargo, 'fmt', '--all', '--check']),
        ('check', [cargo, 'check', '--workspace', '--locked', '--target-dir', 'target/p0-audit']),
        ('clippy', [cargo, 'clippy', '--workspace', '--all-targets', '--locked', '--target-dir', 'target/p0-audit', '--', '-D', 'warnings']),
        ('test', [cargo, 'test', '--workspace', '--locked', '--target-dir', 'target/p0-audit']),
        ('metadata', [cargo, 'metadata', '--locked', '--no-deps', '--format-version', '1']),
    ]
    results = []
    for label, cmd in commands:
        print('START', label, flush=True)
        with (OUT / (label + '.stdout.log')).open('wb') as stdout, (OUT / (label + '.stderr.log')).open('wb') as stderr:
            code = subprocess.run(cmd, cwd=ROOT, env=env, stdout=stdout, stderr=stderr).returncode
        results.append({'check': label, 'command': cmd, 'exit_code': code, 'stdout': label + '.stdout.log', 'stderr': label + '.stderr.log'})
        save('checks.json', results)
        print('END', label, code, flush=True)
    after = manifest()
    save('source-after.json', after)
    before = json.loads((OUT / 'source-before.json').read_text(encoding='utf-8'))
    save('source-comparison.json', {'unchanged': before == after, 'changed': [p for p in sorted(set(before) | set(after)) if before.get(p) != after.get(p)], 'files': len(before)})


if __name__ == '__main__':
    run_checks()
