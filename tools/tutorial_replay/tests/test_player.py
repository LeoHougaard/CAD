import unittest

from tools.tutorial_replay.core import TraceError
from tools.tutorial_replay.player import validate_plan


class VerificationSaveTests(unittest.TestCase):
    def test_verification_save_cannot_escape_owned_output(self):
        plan = {"version": 1, "source": {"width": 1920, "height": 1080}, "actions": []}
        validate_plan({**plan, "verification_save": "receiver-prefix.FCStd"})
        for filename in ("../out.FCStd", "C:\\out.FCStd", "/out.FCStd", ".FCStd", True, None):
            with self.subTest(filename=filename), self.assertRaises(TraceError):
                validate_plan({**plan, "verification_save": filename})


if __name__ == "__main__":
    unittest.main()
