"""Compile against the baseline Cargo build; preserve red target tests."""
from collect import ROOT, OUT, save
import os
from pathlib import Path
import subprocess

rustc = subprocess.check_output(['rustup', 'which', '--toolchain', 'stable', 'rustc'], text=True).strip()
env = dict(os.environ)
env['PATH'] = str(Path(rustc).parent) + os.pathsep + env['PATH']
deps = ROOT / 'target/p0-audit/debug/deps'
cmd = [rustc, '--edition=2021', '--test', str(OUT / 'probe.rs'), '-L', 'dependency='+str(deps), '-o', str(OUT/'probe.exe')]
for name in ('anix_core','anix_domain','anix_agent','anix_integrations','anix_storage','futures','serde_json','tempfile'):
    libs = list(deps.glob('lib'+name+'-*.rlib'))
    if len(libs) != 1:
        raise RuntimeError(f'Ambiguous baseline linkage for {name}: {libs}')
    cmd += ['--extern',name+'='+str(libs[0])]
results=[]
for name, args in [('probe-build',cmd),('probe-target-contracts',[str(OUT/'probe.exe'),'--nocapture','--test-threads=1'])]:
    with (OUT/(name+'.stdout.log')).open('wb') as stdout, (OUT/(name+'.stderr.log')).open('wb') as stderr:
        code=subprocess.run(args,cwd=ROOT,env=env,stdout=stdout,stderr=stderr).returncode
    results.append({'name':name,'command':args,'exit_code':code,'stdout':name+'.stdout.log','stderr':name+'.stderr.log'})
    save('probe-checks.json',results)
    print(name,code,flush=True)
    if name=='probe-build' and code:
        break
