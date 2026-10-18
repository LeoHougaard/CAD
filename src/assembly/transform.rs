use crate::model::{Plane, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Transform {
    pub translation: [f64; 3],
    /// Unit quaternion in x, y, z, w order.
    pub rotation: [f64; 4],
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: [0.; 3],
            rotation: [0., 0., 0., 1.],
        }
    }
}
pub(crate) fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
pub(crate) fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    a.map(|v| v * s)
}
pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn multiply(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}
impl Transform {
    pub fn validate(&self) -> Result<()> {
        if self
            .translation
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 1e8)
            || self.rotation.iter().any(|v| !v.is_finite())
            || (self.rotation.iter().map(|v| v * v).sum::<f64>() - 1.).abs() > 1e-7
        {
            return Err(
                "Invalid rigid transform: require finite translation and unit quaternion".into(),
            );
        }
        Ok(())
    }
    pub fn translated(translation: [f64; 3]) -> Self {
        Self {
            translation,
            ..Self::default()
        }
    }
    pub fn from_euler_degrees(rotation_deg: [f64; 3]) -> Self {
        let [x, y, z] = rotation_deg.map(|v| v.to_radians() / 2.);
        Self {
            rotation: multiply(
                multiply([0., 0., z.sin(), z.cos()], [0., y.sin(), 0., y.cos()]),
                [x.sin(), 0., 0., x.cos()],
            ),
            ..Self::default()
        }
    }
    pub fn vector(&self, v: [f64; 3]) -> [f64; 3] {
        let q = [self.rotation[0], self.rotation[1], self.rotation[2]];
        let t = scale(cross(q, v), 2.);
        add(v, add(scale(t, self.rotation[3]), cross(q, t)))
    }
    pub fn point(&self, p: [f64; 3]) -> [f64; 3] {
        add(self.vector(p), self.translation)
    }
    pub fn compose(&self, other: Self) -> Self {
        let mut rotation = multiply(self.rotation, other.rotation);
        let norm = rotation.iter().map(|v| v * v).sum::<f64>().sqrt();
        rotation.iter_mut().for_each(|v| *v /= norm);
        Self {
            translation: self.point(other.translation),
            rotation,
        }
    }
    pub fn inverse(&self) -> Self {
        let q = self.rotation;
        let mut inverse = Self {
            rotation: [-q[0], -q[1], -q[2], q[3]],
            ..Self::default()
        };
        inverse.translation = inverse.vector(scale(self.translation, -1.));
        inverse
    }
    pub fn plane(&self, p: &Plane) -> Plane {
        Plane {
            origin: self.point(p.origin),
            u: self.vector(p.u),
            v: self.vector(p.v),
            normal: self.vector(p.normal),
        }
    }
    pub fn from_plane(plane: &Plane) -> Result<Self> {
        validate_plane(plane)?;
        let [x, y, z] = [plane.u, plane.v, plane.normal];
        let trace = x[0] + y[1] + z[2];
        let rotation = if trace > 0. {
            let s = (trace + 1.).sqrt() * 2.;
            [
                (y[2] - z[1]) / s,
                (z[0] - x[2]) / s,
                (x[1] - y[0]) / s,
                s / 4.,
            ]
        } else if x[0] > y[1] && x[0] > z[2] {
            let s = (1. + x[0] - y[1] - z[2]).sqrt() * 2.;
            [
                s / 4.,
                (y[0] + x[1]) / s,
                (z[0] + x[2]) / s,
                (y[2] - z[1]) / s,
            ]
        } else if y[1] > z[2] {
            let s = (1. + y[1] - x[0] - z[2]).sqrt() * 2.;
            [
                (y[0] + x[1]) / s,
                s / 4.,
                (z[1] + y[2]) / s,
                (z[0] - x[2]) / s,
            ]
        } else {
            let s = (1. + z[2] - x[0] - y[1]).sqrt() * 2.;
            [
                (z[0] + x[2]) / s,
                (z[1] + y[2]) / s,
                s / 4.,
                (x[1] - y[0]) / s,
            ]
        };
        let transform = Self {
            translation: plane.origin,
            rotation,
        };
        transform.validate()?;
        Ok(transform)
    }
    pub fn approximately(&self, other: &Self) -> bool {
        (0..3).all(|i| (self.translation[i] - other.translation[i]).abs() < 1e-6)
            && self
                .rotation
                .iter()
                .zip(other.rotation)
                .map(|(a, b)| a * b)
                .sum::<f64>()
                .abs()
                > 1. - 1e-8
    }
    pub fn matrix(&self) -> brepkit_math::mat::Mat4 {
        let x = self.vector([1., 0., 0.]);
        let y = self.vector([0., 1., 0.]);
        let z = self.vector([0., 0., 1.]);
        brepkit_math::mat::Mat4([
            [x[0], y[0], z[0], self.translation[0]],
            [x[1], y[1], z[1], self.translation[1]],
            [x[2], y[2], z[2], self.translation[2]],
            [0., 0., 0., 1.],
        ])
    }
}
pub fn validate_plane(p: &Plane) -> Result<()> {
    p.validate()
}
