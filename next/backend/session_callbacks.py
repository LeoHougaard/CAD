"""Shared private worker callback transport for Rust-owned session components."""
import json
import sys

JOURNAL = "cad-next-sketch-journal"
REVISIONS = "cad-next-revisions"
LEASES = "cad-next-leases"
_protocol = None
_request_id = None
_sequence = 0


class CallbackError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code


class CallbackTransportError(BaseException):
    """Unknown callback outcome must terminate the worker, like a broken host pipe."""


def configure(protocol):
    global _protocol
    _protocol = protocol


def request(request_id):
    global _request_id
    _request_id = request_id


def call(channel, action, document, **parameters):
    global _sequence
    _sequence += 1
    frame = dict(kind=channel, version=1, id=_request_id, sequence=_sequence,
                 action=action, document=document, **parameters)
    try:
        _protocol.write(json.dumps(frame, allow_nan=False, separators=(",", ":")) + "\n")
        _protocol.flush()
        reply = json.loads(sys.stdin.readline())
        if any(type(reply.get(key)) is not type(frame[key]) or reply.get(key) != frame[key]
               for key in ("kind", "version", "id", "sequence")):
            raise ValueError("Session callback response correlation mismatch")
        if reply.get("ok") is True and "result" in reply and "error" not in reply:
            return reply["result"]
        if reply.get("ok") is not False or "error" not in reply or "result" in reply:
            raise ValueError("Invalid session callback response envelope")
        code, message = reply["error"]["code"], reply["error"]["message"]
        if not isinstance(code, str) or not isinstance(message, str):
            raise ValueError("Invalid session callback error")
        if code.startswith(("journal_", "revision_", "lease_", "callback_")):
            raise ValueError(message)
    except Exception as error:
        raise CallbackTransportError(str(error)) from error
    raise CallbackError(code, message)
