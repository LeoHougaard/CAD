"""Audit passive native document facts retained by the ownership workflow."""
import argparse
import json
from pathlib import Path


def audit(folder):
    changes, reads, guards, wrong_owner, uuids = [], [], [], [], []
    events = sessions = 0
    for path in sorted(folder.glob("*.jsonl")):
        sessions += 1
        for line_number, line in enumerate(path.read_text().splitlines(), 1):
            event = json.loads(line)
            events += 1
            request = event["request"]
            op, target = request["op"], request.get("document")
            before, after = event["before"], event["after"]
            where = dict(file=path.name, line=line_number, op=op, target=target)
            for doc in before["documents"].keys() & after["documents"].keys():
                if doc != target and before["documents"][doc] != after["documents"][doc]:
                    changes.append(dict(**where, other=doc, before=before["documents"][doc], after=after["documents"][doc]))
                assert "Uid" in before["documents"][doc], "Native UUID diagnostic unavailable"
                if op not in ("new", "open", "close"):
                    assert before["documents"][doc]["Uid"] == after["documents"][doc]["Uid"]
                    uuids.append(before["documents"][doc]["Uid"])
            if op in ("inspect", "mesh", "topology", "measure", "feature_schema", "capabilities"):
                reads.append(dict(**where, unchanged=before == after))
            if event.get("error", {}).get("code") in ("stale_revision", "preview_active", "sketch_active", "sketch_owner", "document_unavailable", "assembly_proxy_restore"):
                guards.append(dict(**where, unchanged=before == after))
            if event["ok"] and op in ("transaction", "preview", "preview_commit", "preview_cancel", "sketch_begin", "sketch_finish", "sketch_cancel", "undo", "redo", "save"):
                if after["active"] != target:
                    wrong_owner.append(dict(**where, active=after["active"]))
    assert sessions == 11 and events > 0, (sessions, events)
    result = dict(sessions=sessions, requests=events, other_native_changes=changes,
                  readonly_requests=len(reads), readonly_all_unchanged=all(r["unchanged"] for r in reads),
                  common_guard_failures=len(guards), guards_all_unchanged=all(g["unchanged"] for g in guards),
                  wrong_active_owner=wrong_owner, native_document_uuids_stable=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--diagnostics", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = audit(args.diagnostics)
    args.output.write_text(json.dumps(result, indent=2)+"\n")
    assert not result["other_native_changes"] and not result["wrong_active_owner"], result
    assert result["readonly_all_unchanged"] and result["guards_all_unchanged"], result
    print(json.dumps(result))


if __name__ == "__main__":
    main()
