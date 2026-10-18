//! Onshape default keys for tools implemented by this application.
//! Keep unimplemented shortcuts unbound rather than assigning them another meaning.
use eframe::egui::{Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    PartStudio,
    Sketch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SketchTool {
    Point,
    Line,
    CornerRectangle,
    CenterRectangle,
    Circle,
    ThreePointArc,
    Trim,
    Dimension,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
    Isometric,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    ClearSelection,
    Cancel,
    Accept,
    AcceptAndRepeat,
    Delete,
    Rename,
    Undo,
    Redo,
    Help,
    ShortcutToolbar,
    SearchTools,
    NewSketch,
    Extrude,
    Revolve,
    Fillet,
    ToggleSketches,
    TogglePlanes,
    HideSelectedOrHovered,
    ShowHidden,
    HideConstruction,
    Fit,
    Normal,
    View(View),
    ZoomIn,
    ZoomOut,
    Rotate {
        horizontal: i8,
        vertical: i8,
        degrees: u8,
    },
    Pan {
        horizontal: i8,
        vertical: i8,
    },
    SketchTool(SketchTool),
    Relationship(&'static str),
    Construction,
    ToggleTangentArc,
}

/// Shift can turn a logical number/slash into punctuation on native backends.
/// Normalize only these physical shortcut keys; letters retain the user's layout.
pub fn command_for_event(
    key: Key,
    physical_key: Option<Key>,
    modifiers: Modifiers,
    context: Context,
) -> Option<Command> {
    let logical_letter = key.name().len() == 1 && key.name().as_bytes()[0].is_ascii_alphabetic();
    let key = if modifiers.shift
        && !logical_letter
        && matches!(
            physical_key,
            Some(
                Key::Num1
                    | Key::Num2
                    | Key::Num3
                    | Key::Num4
                    | Key::Num5
                    | Key::Num6
                    | Key::Num7
                    | Key::Slash
            )
        ) {
        physical_key.unwrap()
    } else {
        key
    };
    command_for(key, modifiers, context)
}

pub fn command_for(key: Key, modifiers: Modifiers, context: Context) -> Option<Command> {
    use Command::*;
    if modifiers.command || modifiers.ctrl || modifiers.mac_cmd {
        if modifiers.alt {
            return None;
        }
        return match (key, modifiers.shift) {
            (Key::Z, false) => Some(Undo),
            (Key::Y, false) | (Key::Z, true) => Some(Redo),
            (Key::ArrowLeft, shift) => Some(if shift {
                Pan {
                    horizontal: -1,
                    vertical: 0,
                }
            } else {
                Rotate {
                    horizontal: -1,
                    vertical: 0,
                    degrees: 5,
                }
            }),
            (Key::ArrowRight, shift) => Some(if shift {
                Pan {
                    horizontal: 1,
                    vertical: 0,
                }
            } else {
                Rotate {
                    horizontal: 1,
                    vertical: 0,
                    degrees: 5,
                }
            }),
            (Key::ArrowUp, shift) => Some(if shift {
                Pan {
                    horizontal: 0,
                    vertical: 1,
                }
            } else {
                Rotate {
                    horizontal: 0,
                    vertical: 1,
                    degrees: 5,
                }
            }),
            (Key::ArrowDown, shift) => Some(if shift {
                Pan {
                    horizontal: 0,
                    vertical: -1,
                }
            } else {
                Rotate {
                    horizontal: 0,
                    vertical: -1,
                    degrees: 5,
                }
            }),
            _ => None,
        };
    }
    if modifiers.alt {
        return (key == Key::C && !modifiers.shift).then_some(SearchTools);
    }
    if modifiers.shift {
        return match key {
            Key::Enter => Some(AcceptAndRepeat),
            Key::N => Some(Rename),
            Key::Slash => Some(Help),
            Key::E => Some(Extrude),
            Key::W => Some(Revolve),
            Key::S if context == Context::PartStudio => Some(NewSketch),
            Key::S if context == Context::Sketch => Some(SketchTool(self::SketchTool::Point)),
            Key::F if context == Context::PartStudio => Some(Fillet),
            Key::H => Some(ToggleSketches),
            Key::Y => Some(ShowHidden),
            Key::P => Some(HideConstruction),
            Key::Num1 => Some(View(self::View::Front)),
            Key::Num2 => Some(View(self::View::Back)),
            Key::Num3 => Some(View(self::View::Left)),
            Key::Num4 => Some(View(self::View::Right)),
            Key::Num5 => Some(View(self::View::Top)),
            Key::Num6 => Some(View(self::View::Bottom)),
            Key::Num7 => Some(View(self::View::Isometric)),
            Key::Z => Some(ZoomIn),
            Key::A if context == Context::Sketch => Some(ToggleTangentArc),
            Key::M if context == Context::Sketch => Some(Relationship("Midpoint")),
            Key::L if context == Context::Sketch => Some(Relationship("Perpendicular")),
            Key::O if context == Context::Sketch => Some(Relationship("Concentric")),
            Key::J if context == Context::Sketch => Some(Relationship("Fix")),
            Key::ArrowLeft => Some(Rotate {
                horizontal: -1,
                vertical: 0,
                degrees: 90,
            }),
            Key::ArrowRight => Some(Rotate {
                horizontal: 1,
                vertical: 0,
                degrees: 90,
            }),
            Key::ArrowUp => Some(Rotate {
                horizontal: 0,
                vertical: 1,
                degrees: 90,
            }),
            Key::ArrowDown => Some(Rotate {
                horizontal: 0,
                vertical: -1,
                degrees: 90,
            }),
            _ => None,
        };
    }
    match key {
        Key::Space => Some(ClearSelection),
        Key::Escape => Some(Cancel),
        Key::Enter => Some(Accept),
        Key::Delete | Key::Backspace => Some(Delete),
        Key::S => Some(ShortcutToolbar),
        Key::P => Some(TogglePlanes),
        Key::Y => Some(HideSelectedOrHovered),
        Key::F => Some(Fit),
        Key::N => Some(Normal),
        Key::Z => Some(ZoomOut),
        Key::ArrowLeft => Some(Rotate {
            horizontal: -1,
            vertical: 0,
            degrees: 15,
        }),
        Key::ArrowRight => Some(Rotate {
            horizontal: 1,
            vertical: 0,
            degrees: 15,
        }),
        Key::ArrowUp => Some(Rotate {
            horizontal: 0,
            vertical: 1,
            degrees: 15,
        }),
        Key::ArrowDown => Some(Rotate {
            horizontal: 0,
            vertical: -1,
            degrees: 15,
        }),
        _ if context == Context::Sketch => match key {
            Key::L => Some(SketchTool(self::SketchTool::Line)),
            Key::G => Some(SketchTool(self::SketchTool::CornerRectangle)),
            Key::R => Some(SketchTool(self::SketchTool::CenterRectangle)),
            Key::C => Some(SketchTool(self::SketchTool::Circle)),
            Key::A => Some(SketchTool(self::SketchTool::ThreePointArc)),
            Key::M => Some(SketchTool(self::SketchTool::Trim)),
            Key::D => Some(SketchTool(self::SketchTool::Dimension)),
            Key::I => Some(Relationship("Coincident")),
            Key::H => Some(Relationship("Horizontal")),
            Key::V => Some(Relationship("Vertical")),
            Key::B => Some(Relationship("Parallel")),
            Key::E => Some(Relationship("Equal")),
            Key::T => Some(Relationship("Tangent")),
            Key::Q => Some(Construction),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tangent_transition_uses_shift_a_only_in_sketch() {
        assert_eq!(
            command_for(Key::A, Modifiers::SHIFT, Context::Sketch),
            Some(Command::ToggleTangentArc)
        );
        assert_eq!(
            command_for(Key::A, Modifiers::SHIFT, Context::PartStudio),
            None
        );
        assert_eq!(
            command_for(Key::A, Modifiers::NONE, Context::Sketch),
            Some(Command::SketchTool(SketchTool::ThreePointArc))
        );
    }
    #[test]
    fn rename_matches_onshape_without_replacing_normal_view() {
        assert_eq!(
            command_for(Key::N, Modifiers::SHIFT, Context::PartStudio),
            Some(Command::Rename)
        );
        assert_eq!(
            command_for(Key::N, Modifiers::NONE, Context::PartStudio),
            Some(Command::Normal)
        );
    }
    #[test]
    fn native_shift_punctuation_uses_physical_view_and_help_keys_only() {
        assert_eq!(
            command_for_event(
                Key::Exclamationmark,
                Some(Key::Num1),
                Modifiers::SHIFT,
                Context::PartStudio
            ),
            Some(Command::View(View::Front))
        );
        assert_eq!(
            command_for_event(
                Key::Questionmark,
                Some(Key::Slash),
                Modifiers::SHIFT,
                Context::PartStudio
            ),
            Some(Command::Help)
        );
        assert_eq!(
            command_for_event(
                Key::Num1,
                Some(Key::Num1),
                Modifiers::NONE,
                Context::PartStudio
            ),
            None
        );
        assert_eq!(
            command_for_event(Key::C, Some(Key::Q), Modifiers::ALT, Context::PartStudio),
            Some(Command::SearchTools)
        );
        assert_eq!(
            command_for_event(Key::L, Some(Key::Q), Modifiers::NONE, Context::Sketch),
            Some(Command::SketchTool(SketchTool::Line))
        );
        assert_eq!(
            command_for_event(Key::L, Some(Key::Slash), Modifiers::SHIFT, Context::Sketch),
            Some(Command::Relationship("Perpendicular"))
        );
    }
    #[test]
    fn sketch_keys_do_not_collide_with_part_studio_tools_or_typing_modifiers() {
        assert_eq!(
            command_for(Key::V, Modifiers::NONE, Context::Sketch),
            Some(Command::Relationship("Vertical"))
        );
        assert_eq!(
            command_for(Key::G, Modifiers::NONE, Context::Sketch),
            Some(Command::SketchTool(SketchTool::CornerRectangle))
        );
        assert_eq!(
            command_for(Key::R, Modifiers::NONE, Context::Sketch),
            Some(Command::SketchTool(SketchTool::CenterRectangle))
        );
        assert_eq!(command_for(Key::C, Modifiers::CTRL, Context::Sketch), None);
        assert_eq!(
            command_for(Key::L, Modifiers::SHIFT, Context::Sketch),
            Some(Command::Relationship("Perpendicular"))
        );
        assert_eq!(
            command_for(Key::S, Modifiers::SHIFT, Context::Sketch),
            Some(Command::SketchTool(SketchTool::Point))
        );
        assert_eq!(command_for(Key::F, Modifiers::SHIFT, Context::Sketch), None);
        assert_eq!(
            command_for(Key::M, Modifiers::SHIFT, Context::Sketch),
            Some(Command::Relationship("Midpoint"))
        );
        assert_eq!(command_for(Key::M, Modifiers::SHIFT, Context::PartStudio), None);
        assert_eq!(
            command_for(Key::S, Modifiers::SHIFT, Context::PartStudio),
            Some(Command::NewSketch)
        );
        assert_eq!(
            command_for(Key::E, Modifiers::SHIFT, Context::Sketch),
            Some(Command::Extrude)
        );
    }
    #[test]
    fn view_keys_use_onshape_numbers_and_arrow_step_sizes() {
        assert_eq!(
            command_for(Key::Num1, Modifiers::NONE, Context::PartStudio),
            None
        );
        assert_eq!(
            command_for(Key::Num1, Modifiers::SHIFT, Context::PartStudio),
            Some(Command::View(View::Front))
        );
        for (modifiers, degrees) in [
            (Modifiers::NONE, 15),
            (Modifiers::CTRL, 5),
            (Modifiers::SHIFT, 90),
        ] {
            assert_eq!(
                command_for(Key::ArrowRight, modifiers, Context::PartStudio),
                Some(Command::Rotate {
                    horizontal: 1,
                    vertical: 0,
                    degrees
                })
            );
        }
    }
    #[test]
    fn visibility_keys_preserve_modifiers_and_tab_remains_field_navigation() {
        for context in [Context::PartStudio, Context::Sketch] {
            assert_eq!(
                command_for(Key::Y, Modifiers::NONE, context),
                Some(Command::HideSelectedOrHovered)
            );
            assert_eq!(
                command_for(Key::Y, Modifiers::SHIFT, context),
                Some(Command::ShowHidden)
            );
            assert_eq!(
                command_for(Key::P, Modifiers::SHIFT, context),
                Some(Command::HideConstruction)
            );
            assert_eq!(
                command_for(Key::Y, Modifiers::CTRL, context),
                Some(Command::Redo)
            );
            assert_eq!(command_for(Key::Tab, Modifiers::NONE, context), None);
        }
    }
}
