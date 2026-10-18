# Numeric expressions

Dimensions, feature number fields and document variables use one evaluator. Expressions remain stored as text and regenerate after variable edits; they are not replaced by rounded display values.

New Part Studio documents use format 2 and Onshape-style degree trigonometry. These examples work in numeric fields:

| Expression | Result |
| --- | --- |
| `#width/2 + 3 mm` | Half the named width plus 3 mm |
| `sin(30)` / `cos(60 deg)` | 0.5 |
| `asin(0.5)` / `atan2(1,-1)` | 30 / 135 degrees |
| `sqrt(25)` / `root(81,4)` / `cbrt(-8)` | 5 / 3 / -2 |
| `2^3^2` / `-2^2` / `2^-2` | 512 / -4 / 0.25 |
| `2 x 3` / `2×3` / `2*3` | 6 |
| `2(#width+5)` | Twice width plus 5 |
| `1/2 inch` / `10 mm` | 12.7 / 10 mm |
| `sin(pi/2 rad)` | 1 |

Use parentheses for function arguments. `root(value)` is square root; `root(value,degree)` is an nth root. The square-root symbol also accepts parentheses, such as `√(25)`. Named variables may use `#width` or `width`. `#x` always means the variable x; a separate x between operands means multiplication. Units are mm, cm, m, inch, deg and rad. π, pi, PI and e are constants. Lengths evaluate in millimeters and angles in degrees. This is a scalar calculator, without dimensional type checking.

Supported functions include sin, cos, tan, asin, acos, atan, atan2, sqrt, root, cbrt, pow, abs, ceil, floor, round, exp, log, ln, log10, min and max. Hyperbolic functions use their ordinary dimensionless mathematical definitions. Explicit sin_rad, cos_rad and tan_rad accept radians.

The degree functions and arithmetic follow [Onshape numeric fields](https://cad.onshape.com/help/Content/Home/numeric_fields.htm). The extra multiplication symbols and root aliases are local conveniences. Expression length and nesting are bounded. Unknown variables, cycles, invalid real domains and nonfinite or out-of-range final values are rejected. Invalid dimension drafts retain their text and leave the previous sketch unchanged.

Format 1 documents keep their earlier Rhai expression semantics, including radians for trig functions. Saving them preserves that format. Format 2 is explicit so older applications reject it rather than evaluating its geometry with different mathematics. The geometry cache includes the expression format. Feature-package scripts retain their own Rhai integer and math semantics; only numeric input expressions use the calculator syntax.
