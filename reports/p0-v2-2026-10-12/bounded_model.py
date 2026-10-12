from dataclasses import dataclass, replace
from collections import deque
import json
# Purpose: inspect a finite DESIGN model, never execute or emulate Rust reducers.
# Per op: phase 0 absent, 1 queued, 2 sent, 3 unknown, 4 received;
# basis revision/epoch; accepted bool. Two required, non-replayable operations.
@dataclass(frozen=True)
class S:
    mode: str='active'
    rev: int=0
    epoch: int=0
    grant: int=-1
    ops: tuple=((0,-1,-1,False),(0,-1,-1,False))
    paid: int=0

def put(s,i,op,**kw):
    ops=list(s.ops); ops[i]=op
    return replace(s,ops=tuple(ops),**kw)

def successors(s,bug=None):
    out=[]
    if s.mode=='closed': return out
    if s.grant!=s.rev: out.append(('approve',replace(s,grant=s.rev)))
    if s.rev==0:
        out.append(('revise',replace(s,rev=1,ops=tuple((p,r,e,False) for p,r,e,a in s.ops))))
    if s.mode=='active': out.append(('pause',replace(s,mode='paused')))
    if s.mode=='paused': out.append(('resume',replace(s,mode='active')))
    if s.mode!='cancelled' and s.epoch==0:
        out.append(('cancel',replace(s,mode='cancelled',epoch=1,ops=tuple((p,r,e,False) for p,r,e,a in s.ops))))
    if s.mode=='cancelled': out.append(('reopen',replace(s,mode='active')))
    for i,(p,r,e,a) in enumerate(s.ops):
        if p==0 and s.mode=='active' and s.grant==s.rev:
            out.append((f'admit{i}',put(s,i,(1,s.rev,s.epoch,False))))
        if p==1 and s.mode=='active' and s.grant==s.rev and r==s.rev and e==s.epoch:
            out.append((f'send{i}',put(s,i,(2,r,e,False))))
        if p==2: out.append((f'crash{i}',put(s,i,(3,r,e,False))))
        if p in (2,3):
            out.append((f'receive{i}',put(s,i,(4,r,e,False),paid=s.paid|(1<<i))))
        if p==4:
            # Duplicate receipts do not change charge or logical outcome.
            out.append((f'duplicate{i}',s))
            if not a and s.mode=='active' and (bug=='no_fence' or (r==s.rev and e==s.epoch)):
                out.append((f'accept{i}',put(s,i,(p,r,e,True))))
    if s.mode=='active' and (bug=='weak_close' or all(p==4 and a and r==s.rev and e==s.epoch for p,r,e,a in s.ops)):
        out.append(('close',replace(s,mode='closed')))
    return out

def invariant(s):
    for i,(p,r,e,a) in enumerate(s.ops):
        if a and not(p==4 and r==s.rev and e==s.epoch): return 'stale acceptance'
        if bool(s.paid&(1<<i))!=(p==4): return 'receipt accounting'
    if s.mode=='closed' and not all(p==4 and a and r==s.rev and e==s.epoch for p,r,e,a in s.ops): return 'premature close'

def run(bug=None):
    start=S(); q=deque([start]); path={start:[]}; edges=0; closed=None; late=None
    while q:
        s=q.popleft()
        problem=invariant(s)
        if problem: return {'bug':bug,'failure':problem,'trace':path[s],'states_seen':len(path)}
        if s.mode=='closed' and closed is None: closed=path[s]
        if s.mode=='cancelled' and any(p==4 for p,r,e,a in s.ops) and late is None: late=path[s]
        for label,t in successors(s,bug):
            edges+=1
            if t not in path: path[t]=path[s]+[label]; q.append(t)
    return {'bug':bug,'states':len(path),'transitions':edges,'violations':0,'positive_closure':closed,'late_receipt_after_cancel':late}
print(json.dumps([run(),run('no_fence'),run('weak_close')],indent=2))
