Derived from brepkit-sketch 4.2.2, https://github.com/andymai/brepkit, AGPL-3.0-only. Original source and tests are retained. Local changes add sketch dimension and relationship constraints. See the repository LICENSE for the license text.

Local solver changes use rank-aware minimum-norm QR steps, wrapped directed-angle residuals, and temporary drag targets projected into the hard-constraint null space. These targets are not stored and do not change DOF analysis. No native library or new dependency is required.
