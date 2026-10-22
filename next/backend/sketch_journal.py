"""Private callback transport. Accepted actions and replay decisions live in Rust."""
import json
import sys

CHANNEL = "cad-next-sketch-journal"
_protocol = None
_request_id = None
_sequence = 0


class JournalError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


class JournalTransportError(BaseException):
    """Unknown journal outcome must terminate the worker, like a broken host pipe."""


def configure(protocol):
    global _protocol
    _protocol = protocol


def request(request_id):
    global _request_id
    _request_id = request_id


def call(action, document, **parameters):
    global _sequence
    _sequence += 1
    frame = dict(kind=CHANNEL, version=1, id=_request_id, sequence=_sequence,
                 action=action, document=document, **parameters)
    _protocol.write(json.dumps(frame, allow_nan=False, separators=(",", ":")) + "\n")
    _protocol.flush()
    try:
        reply = json.loads(sys.stdin.readline())
        if any(reply.get(key) != frame[key] for key in ("kind", "version", "id", "sequence")):
            raise ValueError("Journal callback response correlation mismatch")
        if reply.get("ok") is True and "result" in reply and "error" not in reply:
            return reply["result"]
        if reply.get("ok") is not False or "error" not in reply or "result" in reply:
            raise ValueError("Invalid journal callback response envelope")
        code, message = reply["error"]["code"], reply["error"]["message"]
        if code.startswith("journal_"):
            raise ValueError(message)
    except Exception as error:
        raise JournalTransportError(str(error)) from error
    raise JournalError(code, message)
