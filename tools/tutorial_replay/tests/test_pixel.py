import unittest

from tools.tutorial_replay.adapters.pixel import preflight, to_action
from tools.tutorial_replay.core import TraceError


class PixelTranslationTests(unittest.TestCase):
    window = dict(id="0x123", x=138, y=31, width=2147, height=1017)

    def test_physical_letter_labels_do_not_add_shift(self):
        self.assertEqual(to_action(dict(kind="key", key="S"), self.window)["key"], "s")
        self.assertEqual(to_action(dict(kind="key", key="S", modifiers=["ShiftLeft"]), self.window)["key"], "Shift_L+s")
        self.assertEqual(to_action(dict(kind="key", key="Num6", text="6"), self.window)["key"], "KP_6")

    def test_window_coordinates_and_button_map(self):
        event = dict(kind="click", button="right", target=dict(kind="pixel", xy=[100, 200]))
        self.assertEqual(to_action(event, self.window, "window-client"), dict(action="click", window="0x123", x=238, y=231, button=3))

    def test_function_key_chord_preserves_keysym_case(self):
        self.assertEqual(to_action(dict(kind="key", key="F4", modifiers=["AltLeft"]), self.window)["key"], "Alt_L+F4")
        for key in ("f4", "F0", "F25"):
            with self.subTest(key=key), self.assertRaisesRegex(TraceError, "unsupported Pixel key"):
                to_action(dict(kind="key", key=key), self.window)

    def test_pointer_modifiers_are_never_dropped(self):
        for kind in ("click", "move"):
            with self.subTest(kind=kind), self.assertRaisesRegex(TraceError, "cannot preserve held modifiers"):
                to_action(dict(kind=kind, button="left", modifiers=["ShiftLeft"], target=dict(kind="pixel", xy=[500, 400])), self.window)

    def test_fragment_preflight_rejects_later_unsupported_input(self):
        with self.assertRaisesRegex(TraceError, "does not implement drag"):
            preflight([dict(kind="key", key="S"), dict(kind="drag")], self.window)

    def test_no_path_or_modifier_hold_substitution(self):
        for kind in ("drag", "key_down", "key_up", "text", "scroll"):
            with self.subTest(kind=kind), self.assertRaisesRegex(TraceError, "does not implement"):
                to_action(dict(kind=kind), self.window)
        with self.assertRaisesRegex(TraceError, "outside"):
            to_action(dict(kind="click", button="left", target=dict(kind="pixel", xy=[0, 0])), self.window)
        with self.assertRaisesRegex(TraceError, "rounded"):
            to_action(dict(kind="click", button="left", target=dict(kind="pixel", xy=[2284.9, 100])), self.window)


if __name__ == "__main__":
    unittest.main()
