import copy
import unittest

from tools.tutorial_replay.core import TraceError, compile_plan, map_point, validate


class MappingTests(unittest.TestCase):
    def trace(self, events):
        return {"version": 1, "source": {"width": 1920, "height": 1080}, "events": events}

    def event(self, **values):
        return dict(id="event", t=1., kind="click", status="approved", xy=[500, 200], binding="canvas", **values)

    def mapping(self):
        return {"version": 1, "target": {"width": 1280, "height": 850}, "bindings": {"canvas": {"kind": "rect", "source": [400, 100, 1000, 800], "target": [200, 50, 500, 400]}}}

    def test_rect_preserves_position_within_region(self):
        result = compile_plan(self.trace([self.event()]), self.mapping())
        self.assertEqual(result["actions"][0]["target"], {"kind": "pixel", "xy": [250., 100.]})
        self.assertFalse(result["claims"]["tutorial_pass"])

    def test_native_entity_mapping_keeps_identity_for_live_resolution(self):
        binding = {"kind": "sketch-element", "persistent_id": 10, "point": 0, "fraction": .42}
        mapping = self.mapping()
        mapping["bindings"]["canvas"] = binding
        self.assertEqual(compile_plan(self.trace([self.event()]), mapping)["actions"][0]["target"], binding)

    def test_scene_entity_mapping_keeps_explicit_owner_and_axis_identity(self):
        binding = {"kind": "sketch-element", "persistent_id": 8, "point": 0,
                   "fraction": .75, "object_label": "MASTER LAYOUT", "construction_axis": True}
        mapping = self.mapping()
        mapping["bindings"]["canvas"] = binding
        self.assertEqual(compile_plan(self.trace([self.event()]), mapping)["actions"][0]["target"], binding)

    def test_scene_entity_mapping_rejects_missing_or_ambiguous_owner(self):
        binding = {"kind": "sketch-element", "persistent_id": 8, "point": 0,
                   "object_label": "MASTER LAYOUT", "construction_axis": True}
        for bad in ({**binding, "object_label": ""}, {**binding, "object_label": False},
                    {**binding, "construction_axis": "true"}, {**binding, "point": 1},
                    {k: v for k, v in binding.items() if k != "object_label"}):
            mapping = self.mapping()
            mapping["bindings"]["canvas"] = bad
            with self.subTest(bad=bad), self.assertRaises(TraceError):
                compile_plan(self.trace([self.event()]), mapping)

    def test_native_entity_mapping_rejects_ambiguous_identity_or_point(self):
        valid = {"kind": "sketch-element", "persistent_id": 10, "point": 0, "fraction": .5}
        for mutation in ({"persistent_id": True}, {"persistent_id": 0}, {"point": True}, {"point": 3}, {"fraction": 1.1}):
            mapping = self.mapping()
            mapping["bindings"]["canvas"] = {**valid, **mutation}
            with self.subTest(mutation=mutation), self.assertRaises(TraceError):
                compile_plan(self.trace([self.event()]), mapping)

    def test_one_sample_mapped_drag_is_rejected_during_compilation(self):
        event=self.event()
        event.update(kind="drag",path=[{"dt":0.,"xy":[500,200]}])
        with self.assertRaisesRegex(TraceError,"full pointer path"):
            compile_plan(self.trace([event]),self.mapping())

    def test_affine_rotation_and_translation_preserves_every_path_sample(self):
        binding = {"kind": "affine", "pairs": [{"source": [0, 0], "target": [10, 20]}, {"source": [1, 0], "target": [10, 22]}, {"source": [0, 1], "target": [8, 20]}]}
        event = self.event()
        event.update(kind="drag", path=[{"dt": 0, "xy": [0, 0]}, {"dt": .1, "xy": [1, .5]}, {"dt": .2, "xy": [2, 1]}])
        plan = compile_plan(self.trace([event]), {"version": 1, "target": {"width": 1280, "height": 850}, "bindings": {"canvas": binding}})
        self.assertEqual(plan["actions"][0]["target"]["points"], [{"dt": 0, "xy": [10., 20.]}, {"dt": .1, "xy": [9., 22.]}, {"dt": .2, "xy": [8., 24.]}])

    def test_collinear_calibration_rejected(self):
        with self.assertRaisesRegex(TraceError, "collinear"):
            map_point([1, 1], {"kind": "affine", "pairs": [{"source": [i, i], "target": [i, i]} for i in range(3)]})

    def test_unreviewed_missing_binding_and_outside_region_rejected(self):
        event = self.event()
        for mutation, phrase in [({"status": "pending"}, "reviewed"), ({"binding": "missing"}, "binding"), ({"xy": [5, 5]}, "outside")]:
            with self.subTest(mutation=mutation), self.assertRaisesRegex(TraceError, phrase):
                compile_plan(self.trace([{**event, **mutation}]), self.mapping())

    def test_drag_cannot_be_collapsed_to_click_or_two_endpoints(self):
        event = self.event()
        event["kind"] = "drag"
        with self.assertRaisesRegex(TraceError, "full pointer path"):
            compile_plan(self.trace([event]), self.mapping())

    def test_unresolved_camera_gap_blocks_affected_interval(self):
        trace = self.trace([self.event()])
        trace["gaps"] = [{"start": 2, "end": 3, "reason": "unobserved middle-button gesture", "resolved": False}]
        with self.assertRaisesRegex(TraceError, "unresolved"):
            compile_plan(trace, self.mapping())
        self.assertEqual(len(compile_plan(trace, self.mapping(), end=1.5)["actions"]), 1)

    def test_held_keys_must_balance_and_keep_order(self):
        down = dict(id="down", t=1, kind="key_down", key="Control", status="approved")
        up = dict(down, id="up", t=2, kind="key_up")
        with self.assertRaisesRegex(TraceError, "unreleased"):
            compile_plan(self.trace([down]), {"version": 1})
        result = compile_plan(self.trace([down, up]), {"version": 1})
        self.assertEqual([x["id"] for x in result["actions"]], ["down", "up"])
        with self.assertRaisesRegex(TraceError, "without"):
            compile_plan(self.trace([up]), {"version": 1})

    def test_skips_require_a_reason_and_are_audited(self):
        event = {**self.event(), "status": "skipped", "reason": "Presenter zoom annotation outside CAD"}
        plan = compile_plan(self.trace([event]), self.mapping())
        self.assertEqual(plan["actions"], [])
        self.assertEqual(plan["skipped"][0]["reason"], event["reason"])
        event.pop("reason")
        with self.assertRaisesRegex(TraceError, "reason"):
            compile_plan(self.trace([event]), self.mapping())

    def test_labels_and_native_sketch_bindings_resolve_only_in_live_adapter(self):
        event = self.event()
        mapping = {"version": 1, "bindings": {"canvas": {"kind": "label", "text": "Sketch", "scope": "shortcut"}}}
        result = compile_plan(self.trace([event]), mapping)
        self.assertEqual(result["actions"][0]["target"], mapping["bindings"]["canvas"])

    def test_per_event_override_does_not_mutate_trace_or_other_events(self):
        original = self.trace([self.event(), {**self.event(), "id": "second", "t": 2}])
        before = copy.deepcopy(original)
        mapping = self.mapping()
        mapping["overrides"] = {"event": {"kind": "pixel", "xy": [11, 22]}}
        plan = compile_plan(original, mapping)
        self.assertEqual(plan["actions"][0]["target"]["xy"], [11, 22])
        self.assertEqual(plan["actions"][1]["target"]["xy"], [250, 100])
        self.assertEqual(original, before)

    def test_invalid_time_duplicate_ids_and_nan_rejected(self):
        for events in [[self.event(), self.event()], [{**self.event(), "t": float("nan")}], [{**self.event(), "xy": [True, 3]}], [{**self.event(), "t": -1}]]:
            with self.subTest(events=events), self.assertRaises(TraceError):
                validate(self.trace(events))

    def test_path_may_not_escape_replay_interval_or_cross_unresolved_gap(self):
        event = self.event()
        event.update(kind="drag", path=[{"dt": 0, "xy": [500, 200]}, {"dt": 2, "xy": [600, 300]}])
        with self.assertRaisesRegex(TraceError, "outside selected"):
            compile_plan(self.trace([event]), self.mapping(), end=1.5)
        trace = self.trace([event])
        trace["gaps"] = [{"start": 2, "end": 3, "reason": "unknown gesture"}]
        with self.assertRaisesRegex(TraceError, "unresolved"):
            compile_plan(trace, self.mapping())

    def test_malformed_expectations_are_never_silently_ignored(self):
        for value in ([{"/sketch/geometry_count": 99}], {}, {"geometry_count": 2}, "geometry_count"):
            with self.subTest(value=value), self.assertRaisesRegex(TraceError, "expect"):
                validate(self.trace([{**self.event(), "expect": value}]))

    def test_source_and_target_path_bounds_are_checked(self):
        event = self.event()
        event.update(kind="drag", path=[{"dt": 0, "xy": [500, 200]}, {"dt": .1, "xy": [-1, 9999]}])
        with self.assertRaisesRegex(TraceError, "source path"):
            compile_plan(self.trace([event]), self.mapping())
        mapping = self.mapping()
        mapping["overrides"] = {"event": {"kind": "pixel", "xy": [-1, 99999]}}
        with self.assertRaisesRegex(TraceError, "outside target"):
            compile_plan(self.trace([self.event()]), mapping)

    def test_string_false_cannot_resolve_a_gap(self):
        trace = self.trace([self.event()])
        trace["gaps"] = [{"start": 0, "end": 2, "reason": "unknown gesture", "resolved": "false"}]
        with self.assertRaisesRegex(TraceError, "boolean"):
            compile_plan(trace, self.mapping())

    def test_overlapping_inputs_must_not_be_reordered_after_a_path(self):
        drag = {**self.event(), "kind": "drag", "path": [{"dt": 0, "xy": [500, 200]}, {"dt": 2, "xy": [600, 300]}]}
        key = dict(id="key", t=2, kind="key", key="S", status="approved")
        with self.assertRaisesRegex(TraceError, "interleaved"):
            compile_plan(self.trace([drag, key]), self.mapping())


if __name__ == "__main__":
    unittest.main()
