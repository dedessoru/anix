"""Read-only source/history audit and consistent local DB/artifact backup."""
from collect import ROOT, OUT, sha, save, manifest
import hashlib
import json
import re
import shutil
import sqlite3
import subprocess

def inventories():
    current=manifest()
    normalized={p:hashlib.sha256((ROOT/p).read_bytes().replace(b'\r\n',b'\n')).hexdigest() for p in current}
    digest=hashlib.sha256('\n'.join(h+'  '+p for p,h in sorted(normalized.items())).encode()).hexdigest()
    git={p:hashlib.sha256(subprocess.check_output(['git','show','HEAD:'+p],cwd=ROOT).replace(b'\r\n',b'\n')).hexdigest() for p in current}
    save('source-normalized.json',normalized)
    save('astra-comparison.json',{'files':len(current),'raw_digest':hashlib.sha256('\n'.join(h+'  '+p for p,h in sorted(current.items())).encode()).hexdigest(),
        'normalized_digest':digest,'astra_digest':'a68937c8037a48ba71171194772701a94f2205358af417a0299e2aedd963974e',
        'normalized_equal':digest=='a68937c8037a48ba71171194772701a94f2205358af417a0299e2aedd963974e',
        'head_normalized_equal':git==normalized,'crlf_files':[p for p in current if current[p]!=normalized[p]],
        'zip_available':False,'scope':'50 source/SQL/Cargo/toolchain files; not full 1258-file archive equality'})
    save('specification.json',{'path':'ANIX_CORE_REBUILD_PLAN_V2.md','sha256':sha(ROOT/'ANIX_CORE_REBUILD_PLAN_V2.md'),
        'lines':len((ROOT/'ANIX_CORE_REBUILD_PLAN_V2.md').read_text(encoding='utf-8').splitlines()),'sections_read':list(range(1,23))})
    paths=list((ROOT/'config').glob('*.toml'))+list((ROOT/'models').glob('*.toml'))+list((ROOT/'migrations').glob('*.sql'))
    save('profiles-migrations.json',[{'path':p.relative_to(ROOT).as_posix(),'sha256':sha(p),'bytes':p.stat().st_size} for p in sorted(paths)])
    log=(OUT/'test.stdout.log').read_text(encoding='utf-8')
    summaries=[{'passed':int(a),'failed':int(b),'ignored':int(c)} for a,b,c in re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;',log)]
    save('test-summary.json',{'summaries':summaries,'total_passed':sum(x['passed'] for x in summaries),'failed':sum(x['failed'] for x in summaries),
        'ignored':sum(x['ignored'] for x in summaries),'ignored_names':re.findall(r'^test (\S+) \.\.\. ignored, (.*)$',log,re.M),
        'subprocess_fixture':'crash_child_fixture is ignored in ordinary suite, invoked by passing crash_process_checkpoint_resume; not counted as ordinary PASS',
        'discovered_test_attributes':sum((ROOT/p).read_text(encoding='utf-8').count('#[test]') for p in current if p.endswith('.rs'))})
    old=json.loads((ROOT/'reports/p0-2026-10-11/historical-run-inventory.json').read_text(encoding='utf-8'))
    entries=[]
    for row in old:
        p=ROOT/row['path']
        entry={'label':row['label'],'path':row['path'],'exists_now':p.is_file(),'old_sha256':row.get('sha256')}
        if p.is_file():
            entry['sha256_now']=sha(p)
            entry['unchanged_since_old_report']=entry['sha256_now']==row.get('sha256')
            if p.suffix=='.jsonl':
                rows=[json.loads(s) for s in p.read_text(encoding='utf-8-sig').splitlines()]
                entry['turns']=len(rows)
                entry['has_rendered_messages']=all(isinstance(x.get('trace',{}).get('rendered_messages'),list) for x in rows)
                entry['has_raw_completion']=all('raw_completion' in x.get('trace',{}) for x in rows)
                entry['wire_byte_exact']='Not established: stored diagnostic JSON, not HTTP wire capture'
                points=[]
                for n,x in enumerate(rows,1):
                    t=x.get('trace',{}); action=t.get('parsed_action',{})
                    points.append({'turn':n,'action':action,'execution':t.get('execution'),
                        'payload_sha256':hashlib.sha256(json.dumps(t.get('rendered_messages'),ensure_ascii=False,separators=(',',':')).encode()).hexdigest()})
                private=OUT/'private';private.mkdir(exist_ok=True)
                (private/(row['label']+'-historical-points.json')).write_text(json.dumps(points,ensure_ascii=False,indent=2),encoding='utf-8')
        entries.append(entry)
    save('historical-evidence.json',{'entries':entries,'causal_attribution':'Unknown; observational traces do not prove exclusive historical cause',
        'iteration13_8b664':'Not present in provided historical index; iteration12 is not a substitute','live_current_model_runs':'NOT RUN on P0'})

def backup():
    private=OUT/'private';private.mkdir(exist_ok=True)
    destination=private/'baseline-backup'
    destination.mkdir(exist_ok=True)
    src=sqlite3.connect((ROOT/'.anix/state.sqlite').as_uri()+'?mode=ro',uri=True)
    src.execute('BEGIN')
    # Establish a read snapshot before online backup; writers are never modified.
    tables=[x[0] for x in src.execute("select name from sqlite_master where type='table' and name not like 'sqlite_%'")]
    counts={n:src.execute('select count(*) from "'+n+'"').fetchone()[0] for n in tables}
    dst=sqlite3.connect(destination/'state.sqlite')
    src.backup(dst)
    dst.close()
    files=[]
    for artifact_id,rel,expected,size in src.execute('select id,path,content_hash,byte_size from artifacts'):
        p=(ROOT/rel).resolve()
        if not p.is_relative_to(ROOT.resolve()):
            files.append({'artifact_id':artifact_id,'path':rel,'status':'outside_workspace_not_copied'});continue
        if not p.is_file():
            files.append({'artifact_id':artifact_id,'path':rel,'status':'missing'});continue
        target=destination/rel
        target.parent.mkdir(parents=True,exist_ok=True)
        before=sha(p);shutil.copy2(p,target);after=sha(p);copied=sha(target)
        files.append({'artifact_id':artifact_id,'path':rel,'declared_sha256':expected,'actual_sha256':before,'copied_sha256':copied,
             'declared_bytes':size,'actual_bytes':p.stat().st_size,'status':'verified' if before==after==copied==expected and p.stat().st_size==size else 'mismatch'})
    db=sqlite3.connect((destination/'state.sqlite').as_uri()+'?mode=ro',uri=True)
    restore=private/'baseline-restore';restore.mkdir(exist_ok=True)
    restored=sqlite3.connect(restore/'state.sqlite');db.backup(restored);restored.close()
    for file in files:
        if 'copied_sha256' in file:
            target=restore/file['path'];target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(destination/file['path'],target)
    check=sqlite3.connect((restore/'state.sqlite').as_uri()+'?mode=ro',uri=True)
    restored_counts={n:check.execute('select count(*) from "'+n+'"').fetchone()[0] for n in tables}
    restored_ok=all(sha(restore/x['path'])==x['copied_sha256'] for x in files if 'copied_sha256' in x)
    summary={'method':'SQLite online backup API from read snapshot + immutable-reference file copies; pre/post hashes',
       'scope':'.anix/state.sqlite and every referenced artifact, not other .runtime databases nor unrelated raw sources',
       'sqlite_python':sqlite3.sqlite_version,'backup_db_sha256':sha(destination/'state.sqlite'),'table_counts':counts,
       'backup_integrity':db.execute('pragma integrity_check').fetchall(),'backup_foreign_keys':db.execute('pragma foreign_key_check').fetchall(),
       'restore_integrity':check.execute('pragma integrity_check').fetchall(),'restore_foreign_keys':check.execute('pragma foreign_key_check').fetchall(),
       'restore_counts_equal':counts==restored_counts,'restore_artifact_bytes_equal':restored_ok,
       'artifact_count':len(files),'artifact_status_counts':{s:sum(x['status']==s for x in files) for s in set(x['status'] for x in files)},
       'source_snapshot_still_equal':all(src.execute('select count(*) from "'+n+'"').fetchone()[0]==counts[n] for n in tables),
       'migration_rows':src.execute('select * from schema_migrations').fetchall(),
       'limitations':'No new migration; no power-loss test; external effect reconciliation/continuation belongs to later gates'}
    (private/'artifact-backup-manifest.json').write_text(json.dumps(files,indent=2),encoding='utf-8')
    save('backup-summary.json',summary)
    check.close();db.close();src.rollback();src.close()
    print('backup',summary['artifact_status_counts'],flush=True)

if __name__=='__main__':
    inventories()
    backup()
