"""Verify revision admission, multi-document leases, rollback and name reuse.

All operations use the persistent Rust host and pinned native worker. Rejected
writes must preserve the complete snapshot and must not create output files.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from native_workflows import add, objects, rectangle, set_value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", type=Path, required=True)
    parser.add_argument("--runtime", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    runtime = json.loads(args.runtime.read_text(encoding="utf-8"))
    command = [str(args.host.resolve()), "--executable", runtime["executable"], "--script", runtime["script"]]
    for arg in runtime["args"]:
        command.extend(["--arg", arg])
    process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, encoding="utf-8")
    records = []


    def call(op, error=None, **parameters):
        request = dict(version=1,id=len(records)+1,op=op,**parameters)
        process.stdin.write(json.dumps(request)+"\n")
        process.stdin.flush()
        line = process.stdout.readline()
        assert line, f"Host exited {process.poll()} after {request}"
        response = json.loads(line)
        records.append(dict(request=request,response=response))
        assert response["id"] == request["id"] and response["version"] == 1
        if error:
            assert response["ok"] is False and response["error"]["code"] == error, response
            return response["error"]
        assert response["ok"] is True, response
        return response["result"]


    def inspect(doc):
        return call("inspect",document=doc)


    def mutate(op, doc, error=None, **parameters):
        parameters.setdefault("expected_revision", inspect(doc)["revision"])
        return call(op,error,document=doc,**parameters)


    def revision_checks(doc, cases):
        before = inspect(doc)
        for op, parameters in cases:
            call(op,"missing_revision",document=doc,**parameters)
            call(op,"missing_revision",document=doc,expected_revision=None,**parameters)
            for token in [True, False, float(before["revision"]), str(before["revision"]), -1, before["revision"]-1, [], {}]:
                call(op,"stale_revision",document=doc,expected_revision=token,**parameters)
            assert inspect(doc) == before, op


    def lease_checks(doc, other, sketch=False):
        before, foreign = inspect(doc), inspect(other)
        owner_error = "sketch_active" if sketch else "preview_active"
        # Global lease guards precede document existence, revision and token errors.
        writes = ["new", "open", "transaction", "undo", "redo", "save", "close", "preview",
                  "preview_commit", "preview_cancel", "sketch_begin", "sketch_finish", "sketch_cancel", "export_exchange"]
        for op in writes:
            for target in [other, "Missing", None, 17]:
                for revision in [None, 0, foreign["revision"]]:
                    call(op, owner_error, document=target, expected_revision=revision,
                         token=[], sketch_token={}, edits=[], name="DeniedLease", path=str(output/"denied-lease.FCStd"))
            if op not in {"new", "open"}:
                for target in [[], {}]:
                    call(op,"native_error",document=target,expected_revision=0,token=[])
        # Same-document revision checking precedes local gesture/session ownership.
        call("preview_cancel","missing_revision",document=doc,token=[])
        call("preview_cancel","stale_revision",document=doc,expected_revision=0,token=[])
        local = dict(document=doc,expected_revision=before["revision"])
        for op in ["transaction","undo","redo","save","close","sketch_begin","sketch_finish","export_exchange"]:
            call(op,"preview_active",**local,token="wrong",sketch_token="wrong",edits=[])
        if sketch:
            call("sketch_cancel","sketch_owner",**local,token="wrong")
            for op in ["preview","preview_commit","preview_cancel"]:
                call(op,"sketch_owner",**local,token=[],sketch_token="wrong")
                call(op,"invalid_request",**local,token=[],sketch_token="s")
                call(op,"preview_owner",**local,token="wrong",sketch_token="s")
        else:
            call("sketch_cancel","preview_active",**local,token="wrong")
            for op in ["preview","preview_commit","preview_cancel"]:
                call(op,"invalid_request",**local,token=[])
                call(op,"preview_owner",**local,token="wrong")
        assert inspect(doc) == before and inspect(other) == foreign
        assert not (output/"denied-lease.FCStd").exists()


    try:
        call("capabilities")
        call("feature_schema",type="Part::Box")
        a = call("new",name="RevisionA")
        b = call("new",name="RevisionB")
        assert (a["revision"],b["revision"]) == (1,2)
        doc, other = a["document"], b["document"]
        assert inspect(doc)["revision"] == 1
        call("inspect","not_found",document="Missing")
        call("inspect","not_found")
        # Preserve malformed-identifier behavior as well as malformed tokens.
        for identifier in [[], {}]:
            call("inspect","native_error",document=identifier)
        for identifier in [None, True, 17, 1.5]:
            call("inspect","not_found",document=identifier)
        call("new","name_conflict",name=doc)
        call("open","not_found",path=str(output/"missing.FCStd"))
        state = mutate("transaction",doc,label="Deterministic native model",edits=[add("Part::Box","Box"),
            set_value("Box","Length",20),set_value("Box","Width",30),set_value("Box","Height",40),
            add("Sketcher::SketchObject","Sketch"),*rectangle("Sketch")])["state"]
        assert state["revision"] == 3
        revision_checks(doc,[
            ("transaction",dict(edits=[])),("undo",{}),("redo",{}),
            ("save",dict(path=str(output/"missing-denied.FCStd"))),("close",{}),
            ("preview",dict(token="p",edits=[set_value("Box","Length",25)])),
            ("preview_commit",dict(token="p")),("preview_cancel",dict(token="p")),
            ("sketch_begin",dict(object="Sketch",token="s")),
            ("sketch_finish",dict(token="s")),("sketch_cancel",dict(token="s")),
            ("export_exchange",dict(path=str(output/"denied.step"),objects=["Box"]))])
        assert not (output/"missing-denied.FCStd").exists() and not (output/"denied.step").exists()
        before = inspect(doc)
        for token in [None, True, 17, 1.0, [], {}, "", "x"*257, "🦀"*257]:
            mutate("sketch_begin",doc,"invalid_request",object="Sketch",token=token)
            mutate("preview",doc,"invalid_request",token=token,edits=[])
        for token in ["x"*256, "🦀"*256]:
            mutate("sketch_begin",doc,"not_found",object="Missing",token=token)
            mutate("preview_commit",doc,"no_preview",token=token)
        mutate("transaction",doc,"invalid_request",edits=[])
        mutate("transaction",doc,"invalid_request",edits=[dict(op="constraint",object="Sketch",kind="NotAConstraint",args=[])])
        assert inspect(doc) == before
        for op in ["mesh","topology"]:
            result = call(op,document=doc,object="Box")
            assert result["revision"] == before["revision"]
        mutate("export_exchange",doc,path=str(output/"unchanged.step"),objects=["Box"])
        assert inspect(doc) == before
        state = mutate("undo",doc)
        redo_count = state["redo_count"]
        mutate("preview",doc,"preview_redo_conflict",token="p",edits=[add("Part::Box","Candidate")])
        assert inspect(doc) == state
        # Beginning/canceling an empty sketch session preserves native redo.
        # The model was undone to empty, so restore it before starting the session.
        state = mutate("redo",doc)
        mutate("transaction",doc,label="Branch marker",edits=[set_value("Box","Length",25)])
        state = mutate("undo",doc)
        redo_count = state["redo_count"]
        mutate("sketch_begin",doc,object="Sketch",token="empty")
        state = mutate("sketch_cancel",doc,token="empty")
        assert state["redo_count"] == redo_count
        state = mutate("redo",doc)
        # Failed ordinary edits enter rollback, advance revision and preserve geometry.
        old = state["revision"]
        mutate("transaction",doc,"invalid_model",edits=[set_value("Box","Length",80),add("Part::Cut","Invalid")])
        state = inspect(doc)
        assert state["revision"] == old + 1 and "Invalid" not in objects(state)
        assert objects(state)["Box"]["properties"]["Length"] == 25
        call("transaction","stale_revision",document=doc,expected_revision=old,edits=[set_value("Box","Length",2)])
        # Failed first preview bumps but takes no lease.
        old = state["revision"]
        mutate("preview",doc,"invalid_model",token="first",edits=[add("Part::Cut","Invalid")])
        state = inspect(doc)
        assert state["revision"] == old + 1 and state["preview"] is None
        for edit in [dict(op="body_tip",object="Missing",tip=None),dict(kind="body_reorder",object="Missing",target=None,after=True)]:
            mutate("preview",doc,"history_busy",token="p",edits=[edit])
        state = mutate("preview",doc,token="p",edits=[set_value("Box","Length",50)])["state"]
        lease_checks(doc,other)
        old = state["revision"]
        mutate("preview",doc,"invalid_model",token="p",edits=[add("Part::Cut","Invalid")])
        state = inspect(doc)
        assert state["revision"] == old + 1 and state["preview"]["token"] == "p"
        assert objects(state)["Box"]["properties"]["Length"] == 50
        # Worker lease errors precede another document's missing/stale revision.
        call("transaction","preview_active",document=other,edits=[])
        call("transaction","preview_active",document=other,expected_revision=0,edits=[])
        call("new","preview_active",name="LeaseDenied")
        mutate("preview_cancel",doc,token="p")
        mutate("preview",doc,token="p",edits=[set_value("Box","Length",60)])
        mutate("preview_commit",doc,token="p")
        mutate("undo",doc)
        mutate("redo",doc)
        mutate("sketch_begin",doc,object="Sketch",token="s")
        revision_checks(doc,[("transaction",dict(sketch_token="s",edits=[])),
            ("undo",dict(sketch_token="s")),("redo",dict(sketch_token="s")),
            ("preview",dict(sketch_token="s",token="drag",edits=[])),
            ("sketch_finish",dict(token="s")),("sketch_cancel",dict(token="s"))])
        call("transaction","sketch_active",document=other,edits=[])
        before = inspect(doc)
        for operation in ["transaction","preview"]:
            args = dict(sketch_token="s",token="scope") if operation == "preview" else dict(sketch_token="s")
            for edit in [None, [], {}, dict(op="datum",object="Box",index=0,value=20),
                         dict(op="add_object",object="Sketch",type="Part::Box",name="Denied"),
                         dict(op="unknown",object="Sketch")]:
                mutate(operation,doc,"sketch_active",**args,edits=[edit])
            for op in [[],{}]:
                mutate(operation,doc,"native_error",**args,edits=[dict(object="Sketch",op=op)])
            mutate(operation,doc,"invalid_request",**args,edits=[dict(op="constraint",object="Box",kind="Unknown",args=[])])
            assert inspect(doc) == before
        mutate("transaction",doc,sketch_token="s",edits=[dict(op="datum",object="Sketch",index=9,value=50)])
        mutate("undo",doc,sketch_token="s")
        mutate("redo",doc,sketch_token="s")
        mutate("preview",doc,sketch_token="s",token="drag",edits=[dict(op="datum",object="Sketch",index=9,value=60)])
        lease_checks(doc,other,sketch=True)
        mutate("preview_cancel",doc,sketch_token="s",token="drag")
        mutate("preview",doc,sketch_token="s",token="drag",edits=[dict(op="datum",object="Sketch",index=9,value=70)])
        mutate("preview_commit",doc,sketch_token="s",token="drag")
        mutate("sketch_finish",doc,token="s")
        path = output/"saved.FCStd"
        before = inspect(doc)["revision"]
        saved = mutate("save",doc,path=str(path))
        assert saved["revision"] == before + 1
        original = hashlib.sha256(path.read_bytes()).hexdigest()
        old_other_revision = inspect(other)["revision"]
        last = inspect(doc)["revision"]
        mutate("close",doc)
        call("inspect","not_found",document=doc)
        assert inspect(other)["revision"] == old_other_revision
        state = call("new",name=doc)
        assert state["revision"] == last + 1
        call("close","stale_revision",document=doc,expected_revision=before)
        mutate("close",doc)
        reopened = call("open",path=str(path))
        assert reopened["revision"] == last + 2
        assert hashlib.sha256(path.read_bytes()).hexdigest() == original
        mutate("close",reopened["document"])
        mutate("close",other)
        # Closing the last document never resets the worker-wide clock.
        state = call("new",name="AfterAllClosed")
        assert state["revision"] == last + 3
        mutate("close",state["document"])
    finally:
        process.stdin.close()
        assert process.wait(timeout=10) == 0
        (output/"revision-report.json").write_text(json.dumps(records,indent=2)+'\n', encoding="utf-8")

    print(json.dumps({"responses":len(records),"all_public_operations":True,"exact_clock_lifecycle":True}))


if __name__ == "__main__":
    main()
