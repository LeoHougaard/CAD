from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from .core import TraceError, compile_plan, digest, read_json, record_plan, validate, write_json


def main():
    parser = argparse.ArgumentParser(description="Extract, review, map and replay tutorial input without audio.")
    commands = parser.add_subparsers(dest="command", required=True)
    p = commands.add_parser("extract", help="Detect overlay onsets and cursor positions as unapproved proposals")
    p.add_argument("video")
    p.add_argument("--profile", required=True)
    p.add_argument("--output", required=True)
    p.add_argument("--start", type=float, required=True)
    p.add_argument("--end", type=float, required=True)
    p.add_argument("--fps", type=float, default=30)
    p.add_argument("--ffmpeg")
    p.add_argument("--no-cursor", action="store_true")
    p = commands.add_parser("validate")
    p.add_argument("trace")
    p = commands.add_parser("compile", help="Fail closed until every selected event is approved and mapped")
    p.add_argument("trace")
    p.add_argument("mapping")
    p.add_argument("--output", required=True)
    p.add_argument("--start", type=float)
    p.add_argument("--end", type=float)
    p = commands.add_parser("record", help="Dry-run a plan into a ledger; never inject OS input")
    p.add_argument("plan")
    p.add_argument("--output", required=True)
    p = commands.add_parser("replay", help="Run a mapped plan through the source-bound headless Editor adapter")
    p.add_argument("plan")
    p.add_argument("--executable", required=True)
    p.add_argument("--sha256", required=True, help="expected Editor test executable SHA-256")
    p.add_argument("--runtime", required=True)
    p.add_argument("--output", required=True)
    p.add_argument("--timeout", type=float, default=100)
    p = commands.add_parser("review", help="Print the offline review editor path")
    args = parser.parse_args()
    try:
        if args.command == "extract":
            from .vision import extract
            trace = extract(args.video, args.profile, args.output, args.start, args.end, args.fps, args.ffmpeg, not args.no_cursor)
            result = {"frames": trace["extraction"]["frames"], "pending_events": len(trace["events"]), "output": args.output}
        elif args.command == "validate":
            trace = validate(read_json(args.trace))
            result = {"valid": True, "events": len(trace["events"]), "approved": sum(e["status"] == "approved" for e in trace["events"])}
        elif args.command == "compile":
            plan = compile_plan(read_json(args.trace), read_json(args.mapping), args.start, args.end)
            plan["trace_sha256"] = digest(args.trace)
            plan["mapping_sha256"] = digest(args.mapping)
            output = Path(args.output)
            if output.exists():
                raise TraceError("output exists; use a new name to preserve earlier attempts")
            write_json(output, plan)
            result = {"actions": len(plan["actions"]), "skipped": len(plan["skipped"]), "output": args.output}
        elif args.command == "record":
            if Path(args.output).exists():
                raise TraceError("output exists; use a new name")
            write_json(args.output, record_plan(read_json(args.plan)))
            result = {"mode": "dry-run", "executed_ui": False, "output": args.output}
        elif args.command == "replay":
            from .player import replay
            result = replay(args.plan, args.executable, args.sha256, args.runtime, args.output, args.timeout)
            print(json.dumps(result, indent=2))
            if not result["passed"]:
                raise SystemExit(1)
            return
        else:
            result = {"editor": str(Path(__file__).with_name("review.html").resolve()), "audio": "always muted"}
        print(json.dumps(result, indent=2))
    except (TraceError, OSError, KeyError, json.JSONDecodeError) as exc:
        print(f"tutorial-replay: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc


if __name__ == "__main__":
    main()
