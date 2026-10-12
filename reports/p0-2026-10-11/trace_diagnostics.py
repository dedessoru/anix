import json
import hashlib
from collect import ROOT, OUT, save


def main():
    index = json.loads((OUT / 'trace-index.json').read_text(encoding='utf-8'))
    result = []
    summaries = []
    for entry in index:
        rows = [json.loads(s) for s in (ROOT / entry['trace']).read_text(encoding='utf-8-sig').splitlines()]
        points = []
        for n, row in enumerate(rows, 1):
            trace = row['trace']
            model = row['model_invocation']
            user = next((json.loads(m['content']) for m in trace['rendered_messages'] if m['role'] == 'user'), {})
            decision = user.get('decision_state', {})
            point = {'turn': n, 'invocation_id': model.get('id'), 'work_id': row['work_node'].get('id'), 'snapshot_id': row['context_snapshot'].get('id'), 'plan_revision_id': row['context_snapshot'].get('plan_revision_id'), 'parsed_action': trace.get('parsed_action'), 'raw_completion': trace.get('raw_completion'), 'execution': trace.get('execution'), 'available_source_handles': trace.get('available_source_handles'), 'completion': decision.get('completion'), 'children': decision.get('children'), 'branch_children': decision.get('branch_children'), 'schema_action_types': [s.get('properties', {}).get('type', {}) for s in trace.get('action_schema', {}).get('properties', {}).get('action', {}).get('oneOf', [])]}
            points.append(point)
        summaries.append({'label': entry['label'], 'turns': [{'turn': p['turn'], 'work_id': p['work_id'], 'action': p['execution'].get('executed_action') if isinstance(p['execution'], dict) else None, 'status': p['execution'].get('execution_status') if isinstance(p['execution'], dict) else None, 'completion': p['completion'], 'provider_payload_sha256': hashlib.sha256(json.dumps(rows[p['turn']-1]['trace']['rendered_messages'], ensure_ascii=False, separators=(',', ':')).encode()).hexdigest(), 'schema_sha256': hashlib.sha256(json.dumps(rows[p['turn']-1]['trace']['action_schema'], ensure_ascii=False, separators=(',', ':')).encode()).hexdigest(), 'duplicate': (p['execution'] or {}).get('tool_output', {}).get('data', {}).get('kind') == 'existing_work' if (p['execution'] or {}).get('tool_output') else False} for p in points]})
        result.append({'label': entry['label'], 'points': points})
        print(entry['label'])
    # Derived trace fields may contain original inputs. Keep this diagnostic file local.
    save('diagnostic-points.json', result)
    save('frozen-diagnostic-summary.json', summaries)


if __name__ == '__main__':
    main()
