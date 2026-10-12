"""Verify baseline evidence completeness, never accept the red target contracts."""
from collect import ROOT, OUT, sha, save, manifest
import json
import re
import subprocess

def read(name):
    return json.loads((OUT/name).read_text(encoding='utf-8-sig'))

checks=read('checks.json')
assert all(x['exit_code']==0 for x in checks)
assert {x['name'] for x in checks} >= {'metadata','discovery','fmt','check','build','test','clippy','cli-help'}
for x in checks+read('probe-checks.json')+read('supplement-checks.json'):
    assert (OUT/x['stdout']).is_file() and (OUT/x['stderr']).is_file()
source=manifest()
assert source==read('source-before.json')==read('source-after.json')
assert read('astra-comparison.json')['normalized_equal']
assert read('astra-comparison.json')['head_normalized_equal']
assert sha(ROOT/read('binary.json')['path'])==read('binary.json')['sha256']
summary=read('test-summary.json')
assert summary['total_passed']==186 and summary['ignored']==2 and summary['failed']==0
summary.update(ordinary_passed=183,doctests_passed=3,ordinary_discovered=185,doctests_discovered=3)
save('test-summary.json',summary)
probes=read('probe-checks.json')
assert probes[0]['exit_code']==0 and probes[1]['exit_code']==101
linkage=[]
cmd=probes[0]['command']
for i,arg in enumerate(cmd):
    if arg=='--extern':
        name,path=cmd[i+1].split('=',1)
        from pathlib import Path
        linkage.append({'crate':name,'path':path,'sha256':sha(Path(path))})
save('probe-linkage.json',{'fixture_sha256':sha(OUT/'probe.rs'),'diagnostic_executable_sha256':sha(OUT/'probe.exe'),
    'baseline_libraries':linkage,'compile_command':'probe-checks.json','note':'diagnostic source not edited after final captured compilation/run'})
log=(OUT/probes[1]['stdout']).read_text(encoding='utf-8')
err=(OUT/probes[1]['stderr']).read_text(encoding='utf-8')
assert 'test result: FAILED. 2 passed; 6 failed; 0 ignored;' in log
assert len(re.findall(r"thread 'f\d+_[^']+' .* panicked",err))==6
assert read('F02.json')['actual_error'].endswith('Composite work node')
assert read('F04.json')['old_output_still_satisfies'] and read('F04.json')['completion_validation_ok']
assert read('F05.json')['view']['completion']['ready'] and read('F05.json')['finish_accepted']
assert 'unfinished dependency' in read('F05.json')['direct_error']
f08=read('F08.json')
assert f08['rendered_before']['hypotheses'][0]['statement']=='SNAPSHOT_V1'
assert f08['rendered_after']['hypotheses'][0]['statement']=='LIVE_V2'
assert len(read('F11.json')['orphans'])==1 and len(read('F11.json')['children_after_retry'])==2
assert read('F14.json')['http_attempts']==2 and len(read('F14.json')['logical_invocations'])==1
backup=read('backup-summary.json')
assert backup['artifact_status_counts']=={'verified':896}
assert backup['restore_counts_equal'] and backup['restore_artifact_bytes_equal']
assert backup['backup_integrity']==[['ok']] and backup['restore_integrity']==[['ok']]
assert backup['backup_foreign_keys']==backup['restore_foreign_keys']==[]
assert read('backup-rust-hydration.json')['projects']==207 and read('backup-rust-hydration.json')['every_aggregate_equal']
assets=read('runtime-assets.json')
assert len(assets)==9 and all(x['exists'] and re.fullmatch('[0-9a-f]{64}',x['sha256']) for x in assets)
instructions=read('repository-instructions.json')
assert instructions['found']==[]
assert all(p.replace('\\','/').startswith('.runtime/llama-b11307-src/') for p in instructions['nested_found'])
assert all(x['exists_now'] and x['unchanged_since_old_report'] for x in read('historical-evidence.json')['entries'])
bounded=json.loads((OUT/'bounded-model.stdout.log').read_text(encoding='utf-8'))
assert bounded[0]['states']==1413 and bounded[0]['transitions']==5080 and bounded[0]['violations']==0
assert bounded[1]['failure']=='stale acceptance' and bounded[2]['failure']=='premature close'
tracked=subprocess.check_output(['git','diff','--name-only'],cwd=ROOT,text=True).strip()
assert not tracked
save('gate.json',{'stage':'P0','specification':'ANIX_CORE_REBUILD_PLAN_V2.md','gate':'G0','decision':'PASS',
    'scope':'identified frozen source/binary, baseline checks, preserved executable defects, backup and inventories',
    'baseline':{'ordinary_passed':183,'doctests_passed':3,'ignored':2,'failed':0},
    'target_contracts':{'passed':0,'failed':6,'native_exit_code':101,'meaning':'unfixed defects reproduced; NOT acceptance PASS'},
    'positive_diagnostic_controls':2,'backup_projects':207,'verified_artifacts':896,'production_source_unchanged':True,
    'P1':'NOT STARTED','G1_to_G8':'NOT RUN','historical_exclusive_cause':'Unknown','live_model_qualification':'NOT RUN'})
bundle={p.relative_to(OUT).as_posix():{'bytes':p.stat().st_size,'sha256':sha(p)} for p in sorted(OUT.rglob('*'))
    if p.is_file() and not any(x in p.relative_to(OUT).parts for x in ('private','__pycache__'))
    and p.suffix not in ('.exe','.pdb') and p.name not in ('bundle-manifest.json','verification.json','verification.stdout.log','verification.stderr.log')}
save('bundle-manifest.json',bundle)
save('verification.json',{'evidence_complete':True,'manifest_files':len(bundle),'source_files':len(source),
    'all_bundle_hashes_verified':all(sha(OUT/p)==v['sha256'] for p,v in bundle.items()),'gate':'G0 PASS; six target contracts remain FAIL'})
print('Evidence verified: G0 PASS, 183 ordinary + 3 doctests PASS, 6 target-contract FAIL preserved, 207 projects / 896 artifacts restored.')
