"""Additional P0 inventories, bounded design model and exact local assets."""
from collect import ROOT, OUT, sha, save, manifest
import json
import re
import subprocess
import time

results=[]
def run(name,args):
    started=time.time()
    with (OUT/(name+'.stdout.log')).open('wb') as stdout,(OUT/(name+'.stderr.log')).open('wb') as stderr:
        code=subprocess.run(args,cwd=ROOT,stdout=stdout,stderr=stderr).returncode
    results.append({'name':name,'command':args,'exit_code':code,'seconds':time.time()-started,'stdout':name+'.stdout.log','stderr':name+'.stderr.log'})
    save('supplement-checks.json',results)
    return code

spec=(ROOT/'ANIX_CORE_REBUILD_PLAN_V2.md').read_text(encoding='utf-8')
section=spec[spec.index('## 22.'):]
model=re.search(r'```python\n(.*?)\n```',section,re.S).group(1)
(OUT/'bounded_model.py').write_text(model+'\n',encoding='utf-8')
assert run('bounded-model',['python',str(OUT/'bounded_model.py')])==0
assert run('nvidia-smi',['nvidia-smi','--query-gpu=name,driver_version,memory.total','--format=csv'])==0
for command in ('research','eval'):
    assert run('cli-'+command+'-help',[str(ROOT/'target/p0-audit/debug/anix.exe'),command,'--help'])==0
parents=[ROOT,*ROOT.parents]
save('repository-instructions.json',{'checked_paths':[str(p/'AGENTS.md') for p in parents],
     'found':[str(p/'AGENTS.md') for p in parents if (p/'AGENTS.md').is_file()],
     'nested_found':[str(p.relative_to(ROOT)) for p in ROOT.glob('**/AGENTS.md') if '.git' not in p.parts and 'target' not in p.parts]})
assets=[]
for row in json.loads((ROOT/'reports/p0-2026-10-11/runtime-assets.json').read_text(encoding='utf-8')):
    p=ROOT/row['path']
    print('hashing',row['path'],flush=True)
    assets.append({'path':row['path'],'exists':p.is_file(),'bytes':p.stat().st_size if p.is_file() else None,
        'sha256':sha(p) if p.is_file() else None,'runtime_execution':'NOT RUN; local asset identification only'})
    save('runtime-assets.json',assets)
f14=json.loads((OUT/'F14.json').read_text(encoding='utf-8'))
save('diagnostic-profile.json',{'provider':'loopback OpenAI-compatible fake server; no model inference',
    'limits':{'max_strong_model_calls':1,'timeout_seconds':3,'work_budget':{'context_tokens':4096,'max_model_calls':4,'max_tool_calls':8}},
    'responses':['malformed JSON','compact finish_research Blocked response'],
    'http_attempts':f14['http_attempts'],'logical_model_invocations':len(f14['logical_invocations']),
    'causal_auxiliary_model_calls':0,'payloads_and_schema':'F14.json requests; runtime-owned initial/repair payloads',
    'legacy_seed_limits':{'lead_decisions':40,'tool_calls':60},'production_schema_owner':'test_lead.rs step_response_schema; frozen by source digest',
    'actual_loopback_http_metrics':'server requests are authoritative; provider http_metrics stays zero for non-Groq transport'})
current=manifest()
before=json.loads((OUT/'source-before.json').read_text(encoding='utf-8'))
save('source-after.json',current)
save('source-comparison.json',{'unchanged':current==before,'changed':[p for p in set(current)|set(before) if current.get(p)!=before.get(p)]})
save('asset-checks.json',{'command':['python','reports/p0-v2-2026-10-12/supplement.py'],
     'all_asset_hashes_recorded':len(assets),'source_unchanged':current==before})
print('complete',len(assets),'assets',flush=True)
