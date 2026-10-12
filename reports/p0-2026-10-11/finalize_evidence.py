import json
import os
import re
import shutil
import subprocess
from pathlib import Path
from collect import ROOT, OUT, digest, manifest, save


def main():
    envinfo = json.loads((OUT / 'environment.json').read_text(encoding='utf-8'))
    rustc = envinfo['rustc_path']
    bindir = Path(rustc).parent
    env = dict(os.environ, RUSTC=rustc, RUSTDOC=str(bindir / 'rustdoc.exe'), RUSTUP_TOOLCHAIN='stable')
    env['PATH'] = str(bindir) + os.pathsep + env['PATH']
    cmd = [str(bindir / 'cargo.exe'), 'build', '--workspace', '--examples', '--locked', '--target-dir', 'target/p0-audit']
    with (OUT / 'build.stdout.log').open('wb') as stdout, (OUT / 'build.stderr.log').open('wb') as stderr:
        code = subprocess.run(cmd, cwd=ROOT, env=env, stdout=stdout, stderr=stderr).returncode
    save('build.json', {'command': cmd, 'exit_code': code})
    p = ROOT / 'target/p0-audit/debug/examples/lead_runtime_seed.exe'
    save('fresh-runner.json', {'path': str(p), 'exists': p.exists(), 'sha256': digest(p) if p.exists() else None, 'live_model_run': 'NOT RUN: P0 uses frozen failure traces; no profile qualification or paid calls'})
    meta = json.loads((OUT / 'metadata.stdout.log').read_text(encoding='utf-8'))
    save('workspace-inventory.json', {'members': meta['workspace_members'], 'packages': [{'name': p['name'], 'manifest': p['manifest_path'], 'features': p['features'], 'targets': p['targets'], 'dependencies': p['dependencies']} for p in meta['packages']], 'lock_sha256': digest(ROOT / 'Cargo.lock')})
    text = (OUT / 'test.stdout.log').read_text(encoding='utf-8')
    stderr = (OUT / 'test.stderr.log').read_text(encoding='utf-8')
    totals = [tuple(map(int, m)) for m in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;', text)]
    # Three rustdoc tests are the only non-zero results after Doc-tests in stderr.
    tests = [{'symbol': m[0], 'result': m[1]} for m in re.findall(r'^test (\S+) \.\.\. (ok|ignored[^\n]*)', text, re.M)]
    save('test-results.json', {'ordinary_passed': sum(t[0] for t in totals) - 3, 'doc_passed': 3, 'failed': sum(t[1] for t in totals), 'ignored': sum(t[2] for t in totals), 'tests': tests, 'rustdoc_targets_from_stderr': [l.strip() for l in stderr.splitlines() if 'Doc-tests' in l]})
    assets = []
    for p in (ROOT / 'models').rglob('*.gguf'):
        assets.append({'path': p.relative_to(ROOT).as_posix(), 'bytes': p.stat().st_size, 'sha256': 'UNKNOWN: not rehashed in P0; historical expected hashes are not verification'})
    for name in ('.runtime/llama-b11307-cuda-build/bin/Release/llama-server.exe', '.runtime/llama-b11307/llama-server.exe'):
        p = ROOT / name
        assets.append({'path': name, 'exists': p.exists(), 'sha256': digest(p) if p.exists() else None})
    save('runtime-assets.json', assets)
    for name in ('.runtime/anix-audit-test-lead.toml', '.runtime/anix-v1-test-lead-14b.toml'):
        p = ROOT / name
        if p.exists():
            dest = OUT / 'isolated/config' / p.name
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(p, dest)
    before = json.loads((OUT / 'source-before.json').read_text(encoding='utf-8'))
    after = manifest()
    save('source-after.json', after)
    save('source-comparison.json', {'unchanged': before == after, 'changed': [p for p in sorted(set(before) | set(after)) if before.get(p) != after.get(p)], 'files': len(before)})
    print('build', code, 'ordinary', sum(t[0] for t in totals) - 3, 'docs', 3, 'ignored', sum(t[2] for t in totals))


if __name__ == '__main__':
    main()
