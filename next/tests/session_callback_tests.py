"""Broken private callback transport must terminate, not report an ordinary error."""
import importlib.util
import io
import json
from pathlib import Path
import sys
import unittest


class TransportTests(unittest.TestCase):
    def setUp(self):
        path = Path(__file__).resolve().parents[1] / "backend/session_callbacks.py"
        spec = importlib.util.spec_from_file_location("callback_test", path)
        self.module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.module)
        self.output = io.StringIO()
        self.module.configure(self.output)
        self.module.request(-19)
        self.input = sys.stdin

    def tearDown(self):
        sys.stdin = self.input

    def frame(self):
        return dict(kind=self.module.REVISIONS, version=1, id=-19, sequence=1, ok=True, result=7)

    def invoke(self, reply):
        sys.stdin = io.StringIO(json.dumps(reply) + "\n")
        return self.module.call(self.module.REVISIONS, "current", "Doc")

    def test_success_and_ordinary_admission_error(self):
        self.assertEqual(self.invoke(self.frame()), 7)
        sent = json.loads(self.output.getvalue())
        self.assertEqual((sent["kind"], sent["sequence"], sent["action"]), (self.module.REVISIONS, 1, "current"))
        reply = self.frame()
        reply.update(sequence=2, ok=False, error=dict(code="stale_revision", message="Document revision changed"))
        del reply["result"]
        with self.assertRaises(self.module.CallbackError):
            self.invoke(reply)

    def test_correlation_types_and_malformed_envelopes_are_fatal(self):
        for key, value in [("kind", self.module.JOURNAL), ("version", 2), ("version", True),
                           ("id", -18), ("sequence", True), ("sequence", 2), ("ok", 1)]:
            self.module._sequence = 0
            reply = self.frame()
            reply[key] = value
            with self.subTest(key=key, value=value), self.assertRaises(self.module.CallbackTransportError):
                self.invoke(reply)

    def test_internal_errors_malformed_errors_and_eof_are_fatal(self):
        for error in [dict(code="revision_state", message="Missing state"),
                      dict(code="journal_state", message="Missing state"),
                      dict(code=17, message="Bad code"), dict(code="stale_revision", message=17)]:
            self.module._sequence = 0
            reply = self.frame()
            reply.update(ok=False, error=error)
            del reply["result"]
            with self.assertRaises(self.module.CallbackTransportError):
                self.invoke(reply)
        sys.stdin = io.StringIO("")
        with self.assertRaises(self.module.CallbackTransportError):
            self.module.call(self.module.REVISIONS, "current", "Doc")

    def test_pipe_write_failure_is_fatal(self):
        self.output.close()
        with self.assertRaises(self.module.CallbackTransportError):
            self.module.call(self.module.REVISIONS, "bump", "Doc")


if __name__ == "__main__":
    unittest.main()
