import math
import unittest

from tools.tutorial_replay.alignment import (
    fit_similarity,
    fit_translation,
    inverse_transform_point,
    propose_camera_step,
    transform_point,
)


class AlignmentTests(unittest.TestCase):
    def test_translation_and_residuals(self):
        result = fit_translation([(0, 0), (20, 10), (4, 3)],
                                 [(7, -3), (27, 7), (11, 0)])
        self.assertAlmostEqual(result["transform"]["translation"][0], 7)
        self.assertAlmostEqual(result["transform"]["translation"][1], -3)
        self.assertAlmostEqual(result["rmse_pixels"], 0)
        self.assertAlmostEqual(result["max_residual_pixels"], 0)
        self.assertEqual(len(result["residuals"]), 3)

    def test_similarity_with_rotation_and_noise(self):
        source = [(0, 0), (10, 0), (0, 10), (10, 10)]
        target = [(30.2, -3.9), (29.8, 15.9), (9.8, -4.1), (10.2, 16.1)]
        result = fit_similarity(source, target)
        transform = result["transform"]
        self.assertAlmostEqual(transform["scale"], 2)
        self.assertAlmostEqual(transform["rotation_radians"], math.pi / 2)
        self.assertAlmostEqual(transform["translation"][0], 30)
        self.assertAlmostEqual(transform["translation"][1], -4)
        self.assertAlmostEqual(result["rmse_pixels"], math.sqrt(.05))
        self.assertAlmostEqual(result["max_residual_pixels"], math.sqrt(.05))
        self.assertAlmostEqual(result["residuals"][0]["dx"], -.2)
        self.assertAlmostEqual(result["residuals"][0]["dy"], -.1)
        held_out = transform_point(transform, (3, 8))
        self.assertAlmostEqual(held_out[0], 14)
        self.assertAlmostEqual(held_out[1], 2)
        original = inverse_transform_point(transform, held_out)
        self.assertAlmostEqual(original[0], 3)
        self.assertAlmostEqual(original[1], 8)

    def test_camera_step_and_unsupported_rotation(self):
        source = [(0, 0), (10, 0), (0, 10)]
        desired = [(3, 4), (23, 4), (3, 24)]
        step = propose_camera_step(source, desired, (5, 6))
        self.assertEqual(step["status"], "proposed")
        self.assertEqual(step["input_basis"], "inferred")
        self.assertAlmostEqual(step["wheel_delta"], math.log(2) / .002)
        self.assertAlmostEqual(step["post_zoom_pan"][0], 8)
        self.assertAlmostEqual(step["post_zoom_pan"][1], 10)
        rotated = propose_camera_step(source, [(0, 0), (0, 10), (-10, 0)], (5, 6))
        self.assertEqual(rotated["status"], "unsupported_rotation")
        self.assertNotIn("wheel_delta", rotated)
        self.assertNotIn("post_zoom_pan", rotated)

    def test_invalid_matches_and_degenerate_fits(self):
        good = [(0, 0), (1, 1)]
        bad_cases = [
            ([(0, 0)], [(1, 1)]),
            (good, [(1, 1)]),
            ([(0, 0), (0, 0)], good),
            (good, [(1, 1), (1, 1)]),
            ([(0, 0), (True, 1)], good),
            ([(0, 0), (float("nan"), 1)], good),
            ([(0, 0), (float("inf"), 1)], good),
            ([(0, 0), ("1", 1)], good),
            ([(0, 0), (1, 1, 2)], good),
        ]
        for source, target in bad_cases:
            for fit in (fit_translation, fit_similarity):
                with self.subTest(source=source, target=target, fit=fit.__name__):
                    with self.assertRaises(ValueError):
                        fit(source, target)
        # Reflection of a symmetric point set has zero best-fit similarity scale.
        square = [(-1, -1), (1, -1), (-1, 1), (1, 1)]
        with self.assertRaises(ValueError):
            fit_similarity(square, [(1, -1), (-1, -1), (1, 1), (-1, 1)])
        with self.assertRaises(ValueError):
            propose_camera_step(good, good, (0, 0), True)
        with self.assertRaises(ValueError):
            propose_camera_step(good, good, (0, 0), -1)


if __name__ == "__main__":
    unittest.main()

