fn build(p) {
 if p.inner <= 0.0 || p.outer <= p.inner { throw "Outer radius must be larger than the positive bore radius"; }
 [#{op:"cylinder", radius:p.outer, height:p.height, mode:"New"},
  #{op:"cylinder", radius:p.inner, height:p.height+2.0, z:-1.0, mode:"Remove", target:"@last"}]
}
