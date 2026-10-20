fn build(p) {
    if p.mode == "Cut" {
        #{op:"extrude", sketch:p.profile, target:p.target, depth:p.cut_depth, mode:"Remove"}
    } else {
        #{op:"box", width:p.width, depth:5.0, height:p.height, x:40.0, mode:"New"}
    }
}
