"""V2 P0: frozen-source baseline, native exit codes and local evidence only."""
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent

def sha(path):
    return hashlib.file_digest(path.open('rb'), 'sha256').hexdigest()

def save(name, data):
    (OUT / name).write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding='utf-8')

def manifest():
    paths = list((ROOT / 'crates').rglob('*.rs')) + list((ROOT / 'tests').glob('*.rs'))
    paths += list((ROOT / 'migrations').glob('*.sql')) + list((ROOT / 'crates').rglob('Cargo.toml'))
    paths += [ROOT / n for n in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml')]
    return {p.relative_to(ROOT).as_posix(): sha(p) for p in sorted(paths)}

def main():
    before = manifest()
    save('source-before.json', before)
    rows = ['%s  %s' % (h, p) for p, h in sorted(before.items())]
    digest = hashlib.sha256('\n'.join(rows).encode()).hexdigest()
    save('astra-comparison.json', {'files': len(before), 'current_digest': digest,
        'astra_digest': 'a68937c8037a48ba71171194772701a94f2205358af417a0299e2aedd963974e',
        'equal': digest == 'a68937c8037a48ba71171194772701a94f2205358af417a0299e2aedd963974e'})
    rustc = subprocess.check_output(['rustup', 'which', '--toolchain', 'stable', 'rustc'], text=True).strip()
    bindir = Path(rustc).parent
    env = dict(os.environ, RUSTC=rustc, RUSTDOC=str(bindir / 'rustdoc.exe'), RUSTUP_TOOLCHAIN='stable')
    env['PATH'] = str(bindir) + os.pathsep + env['PATH']
    cargo = str(bindir / 'cargo.exe')
    save('environment.json', {'os': platform.platform(), 'python': platform.python_version(),
         'rustc_path': rustc, 'target_dir': 'target/p0-audit', 'network_model_calls': 0})
    commands = [('git-head', ['git', 'rev-parse', 'HEAD']), ('git-status', ['git', 'status', '--porcelain']),
        ('git-diff', ['git', 'diff', '--binary']), ('rustc-version', [rustc, '--version', '--verbose']),
        ('cargo-version', [cargo, '--version']),
        ('metadata', [cargo, 'metadata', '--locked', '--no-deps', '--format-version', '1']),
        ('discovery', [cargo, 'test', '--workspace', '--locked', '--target-dir', 'target/p0-audit', '--', '--list']),
        ('fmt', [cargo, 'fmt', '--all', '--', '--check']),
        ('check', [cargo, 'check', '--workspace', '--all-targets', '--locked', '--target-dir', 'target/p0-audit']),
        ('build', [cargo, 'build', '--workspace', '--locked', '--target-dir', 'target/p0-audit']),
        ('test', [cargo, 'test', '--workspace', '--locked', '--target-dir', 'target/p0-audit']),
        ('clippy', [cargo, 'clippy', '--workspace', '--all-targets', '--locked', '--target-dir', 'target/p0-audit', '--', '-D', 'warnings']),
        ('cli-help', [str(ROOT / 'target/p0-audit/debug/anix.exe'), '--help'])]
    results = []
    for name, cmd in commands:
        print('START', name, flush=True)
        started = time.time()
        with (OUT / (name + '.stdout.log')).open('wb') as stdout, (OUT / (name + '.stderr.log')).open('wb') as stderr:
            code = subprocess.run(cmd, cwd=ROOT, env=env, stdout=stdout, stderr=stderr).returncode
        results.append({'name': name, 'command': cmd, 'exit_code': code, 'seconds': time.time()-started,
                        'stdout': name+'.stdout.log', 'stderr': name+'.stderr.log'})
        save('checks.json', results)
        print('END', name, code, flush=True)
    after = manifest()
    save('source-after.json', after)
    save('source-comparison.json', {'unchanged': before == after, 'changed': [p for p in before if before[p] != after.get(p)]})
    save('binary.json', {'path': 'target/p0-audit/debug/anix.exe', 'sha256': sha(ROOT / 'target/p0-audit/debug/anix.exe')})

if __name__ == '__main__':
    main()
