//! SpaceMouse motion, coalesced into one state. Windows uses foreground Raw
//! Input in the existing event loop: no polling thread, HID library or SDK DLL.
use crate::render::Camera;
use eframe::egui;
use glam::{Quat, Vec3};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Options {
    pub enabled: bool,
    pub speed: f32,
    pub reverse_zoom: bool,
    pub reverse_rotation: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            enabled: true,
            speed: 1.,
            reverse_zoom: false,
            reverse_rotation: false,
        }
    }
}
#[derive(Default)]
struct State {
    rotating: bool,
    translation_motion: Motion,
    rotation_motion: Motion,
    buttons: u32,
    pressed: u32,
    reports: u64,
    device: String,
    error: Option<String>,
    ctx: Option<egui::Context>,
}

// Raw HID reports are velocities, not mouse deltas. Integrate at their own
// timestamps so uneven USB/report and redraw intervals do not change speed.
// A short continuous-time filter removes sensor steps without a report queue,
// polling thread, or a frame-rate-dependent moving average.
const RESPONSE_SECONDS: f32 = 0.012;
const REPORT_TIMEOUT: Duration = Duration::from_millis(150);
const REST_EPSILON: f32 = 0.0001;

#[derive(Default)]
struct Motion {
    target: Vec3,
    velocity: Vec3,
    integral: Vec3,
    at: Option<Instant>,
    report_at: Option<Instant>,
}
impl Motion {
    #[cfg(any(windows, test))]
    fn report(&mut self, axes: [i16; 3], now: Instant) {
        self.advance(now);
        self.target = Vec3::from_array(axes.map(axis));
        self.report_at = Some(now);
    }
    fn integrate(&mut self, target: Vec3, seconds: f32) {
        if seconds <= 0. {
            return;
        }
        let decay = (-seconds / RESPONSE_SECONDS).exp();
        let difference = self.velocity - target;
        // Exact integral of v(t) = target + (v(0) - target) exp(-t / tau).
        self.integral += target * seconds + difference * (RESPONSE_SECONDS * (1. - decay));
        self.velocity = target + difference * decay;
        if target == Vec3::ZERO && self.velocity.abs().max_element() < REST_EPSILON {
            self.velocity = Vec3::ZERO;
        }
    }
    fn advance(&mut self, now: Instant) {
        let Some(previous) = self.at.replace(now) else {
            return;
        };
        let expires = self.report_at.map(|at| at + REPORT_TIMEOUT);
        let fresh_end = expires.map_or(previous, |at| now.min(at).max(previous));
        self.integrate(
            self.target,
            fresh_end.duration_since(previous).as_secs_f32(),
        );
        self.integrate(
            Vec3::ZERO,
            now.saturating_duration_since(fresh_end).as_secs_f32(),
        );
        if expires.is_none_or(|at| now >= at) {
            self.target = Vec3::ZERO;
        }
    }
    fn take(&mut self, now: Instant) -> Vec3 {
        self.advance(now);
        std::mem::take(&mut self.integral)
    }
    fn active(&self) -> bool {
        self.target != Vec3::ZERO || self.velocity != Vec3::ZERO
    }
}
impl State {
    fn clear(&mut self) {
        self.rotating = false;
        self.pressed = 0;
        self.buttons = 0;
        self.translation_motion = Motion::default();
        self.rotation_motion = Motion::default();
    }
    #[cfg(any(windows, test))]
    fn report(&mut self, report: &[u8], now: Instant) -> bool {
        let vector = |offset: usize| -> Option<[i16; 3]> {
            let s = report.get(offset..offset + 6)?;
            Some([
                i16::from_le_bytes([s[0], s[1]]),
                i16::from_le_bytes([s[2], s[3]]),
                i16::from_le_bytes([s[4], s[5]]),
            ])
        };
        match report.first() {
            Some(1) => {
                let Some(v) = vector(1) else { return false };
                self.translation_motion.report(v, now);
                if let Some(r) = vector(7) {
                    self.rotation_motion.report(r, now);
                }
            }
            Some(2) => {
                let Some(v) = vector(1) else { return false };
                self.rotation_motion.report(v, now);
            }
            Some(3) => {
                let Some(bytes) = report.get(1..5) else {
                    return false;
                };
                let buttons = u32::from_le_bytes(bytes.try_into().unwrap());
                self.pressed |= buttons & !self.buttons;
                self.buttons = buttons;
            }
            _ => return false,
        }
        self.reports += 1;
        true
    }
}
#[derive(Clone, Default)]
pub struct SpaceMouse {
    state: Arc<Mutex<State>>,
    pub options: Options,
}
#[cfg(any(windows, test))]
fn axis(value: i16) -> f32 {
    let n = value as f32;
    n.signum() * ((n.abs() - 8.) / 342.).clamp(0., 1.)
}
impl SpaceMouse {
    pub fn status(&self) -> String {
        let state = self.state.lock().unwrap();
        if let Some(error) = &state.error {
            error.clone()
        } else if state.device.is_empty() {
            "Not connected".into()
        } else {
            state.device.clone()
        }
    }
    pub fn reports(&self) -> u64 {
        self.state.lock().unwrap().reports
    }
    pub fn initialize(&self, cc: &eframe::CreationContext<'_>) {
        self.state.lock().unwrap().ctx = Some(cc.egui_ctx.clone());
        #[cfg(windows)]
        windows::register(self, cc);
    }
    #[cfg(windows)]
    pub fn install_hook(&self, builder: &mut eframe::EventLoopBuilder<eframe::UserEvent>) {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let mouse = self.clone();
        let mut input = windows::Input::default();
        builder.with_msg_hook(move |message| {
            // Winit supplies a live Win32 MSG for the duration of this callback.
            unsafe {
                input.message(message.cast(), &mouse);
            }
            false // Winit retains normal dispatch and DefWindowProc cleanup.
        });
    }
    /// Returns a Fit button press. Repaints continue only while motion is fresh.
    pub fn update(
        &self,
        ctx: &egui::Context,
        camera: &mut Camera,
        sketching: bool,
        blocked: bool,
        rotation_pivot: impl FnOnce(&Camera) -> Vec3,
    ) -> bool {
        let now = Instant::now();
        let mut s = self.state.lock().unwrap();
        if !self.options.enabled || blocked || !ctx.input(|i| i.viewport().focused.unwrap_or(false))
        {
            s.clear();
            return false;
        }
        let t = s.translation_motion.take(now);
        let rotation = s.rotation_motion.take(now);
        let r = if sketching { Vec3::ZERO } else { rotation };
        let fit = s.pressed & 2 != 0;
        s.pressed = 0;
        if r != Vec3::ZERO && !s.rotating {
            camera.pivot = rotation_pivot(camera);
        }
        s.rotating = r != Vec3::ZERO;
        if t.length_squared() + r.length_squared() > 0. {
            // These are already time integrals, including filter transitions.
            apply_motion(camera, t, r, 1., &self.options);
        }
        if s.translation_motion.active() || (!sketching && s.rotation_motion.active()) {
            // Let vsync pace active navigation. Adding a 16ms timer after every
            // redraw can miss a refresh and produce an alternating frame cadence.
            ctx.request_repaint();
        }
        fit
    }
}
fn apply_motion(camera: &mut Camera, t: Vec3, r: Vec3, dt: f32, options: &Options) {
    let step = dt * options.speed;
    let zoom_sign = if options.reverse_zoom { -1. } else { 1. };
    // Object mode: raw +X moves right, -Z moves up, +Y moves toward the viewer.
    // This is the HID mapping used by FreeSpacenav's move-object convention.
    let zoom = -t.y * step * zoom_sign * 1.8;
    let average_span = if zoom.abs() < 0.00001 {
        1. + zoom * 0.5
    } else {
        zoom.exp_m1() / zoom
    };
    camera.center +=
        camera.rotation * Vec3::new(-t.x, t.z, 0.) * camera.span * average_span * step * 0.75;
    camera.span = (camera.span * zoom.exp()).clamp(0.02, 1e6);
    let rotation_sign = if options.reverse_rotation { -1. } else { 1. };
    let rotation = Vec3::new(-r.x, r.z, -r.y) * step * rotation_sign * 1.8;
    camera.rotate_to(camera.rotation * Quat::from_scaled_axis(rotation));
}

#[cfg(windows)]
mod windows {
    use super::*;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::{
        collections::HashMap,
        mem::{offset_of, size_of},
        ptr,
    };
    use windows_sys::Win32::UI::{
        Input::*,
        WindowsAndMessaging::{MSG, WM_INPUT, WM_INPUT_DEVICE_CHANGE, WM_KILLFOCUS},
    };
    #[derive(Default)]
    pub struct Input {
        buffer: Vec<u64>,
        devices: HashMap<usize, bool>,
    }
    fn device(handle: windows_sys::Win32::Foundation::HANDLE) -> Option<String> {
        let mut info = RID_DEVICE_INFO {
            cbSize: size_of::<RID_DEVICE_INFO>() as u32,
            ..Default::default()
        };
        let mut size = info.cbSize;
        // The initialized union is read as HID only after checking its tag.
        unsafe {
            if GetRawInputDeviceInfoW(
                handle,
                RIDI_DEVICEINFO,
                (&mut info as *mut RID_DEVICE_INFO).cast(),
                &mut size,
            ) == u32::MAX
                || info.dwType != RIM_TYPEHID
            {
                return None;
            }
            let hid = info.Anonymous.hid;
            if ![0x046d, 0x256f].contains(&hid.dwVendorId)
                || hid.usUsagePage != 1
                || hid.usUsage != 8
            {
                return None;
            }
            Some(
                match hid.dwProductId {
                    0xc62b => "SpaceMouse Pro",
                    0xc626 => "SpaceNavigator",
                    0xc635 => "SpaceMouse Compact",
                    0xc631 | 0xc632 => "SpaceMouse Pro Wireless",
                    _ => "SpaceMouse",
                }
                .into(),
            )
        }
    }
    fn discover(mouse: &SpaceMouse) {
        let mut count = 0;
        unsafe {
            if GetRawInputDeviceList(
                ptr::null_mut(),
                &mut count,
                size_of::<RAWINPUTDEVICELIST>() as u32,
            ) == u32::MAX
                || count > 1024
            {
                return;
            }
            let mut list = vec![RAWINPUTDEVICELIST::default(); count as usize];
            let found = GetRawInputDeviceList(
                list.as_mut_ptr(),
                &mut count,
                size_of::<RAWINPUTDEVICELIST>() as u32,
            );
            if found == u32::MAX {
                return;
            }
            let name = list[..found as usize]
                .iter()
                .filter_map(|d| device(d.hDevice))
                .next()
                .unwrap_or_default();
            let mut state = mouse.state.lock().unwrap();
            state.device = name;
            state.clear();
        }
    }
    pub fn register(mouse: &SpaceMouse, cc: &eframe::CreationContext<'_>) {
        let result = (|| {
            let handle = cc.window_handle().map_err(|e| e.to_string())?;
            let RawWindowHandle::Win32(handle) = handle.as_raw() else {
                return Err("Window handle unavailable".into());
            };
            let registration = RAWINPUTDEVICE {
                usUsagePage: 1,
                usUsage: 8,
                dwFlags: RIDEV_DEVNOTIFY,
                hwndTarget: handle.hwnd.get() as _,
            };
            if unsafe {
                RegisterRawInputDevices(&registration, 1, size_of::<RAWINPUTDEVICE>() as u32)
            } == 0
            {
                return Err(format!("Raw Input: {}", std::io::Error::last_os_error()));
            }
            Ok(())
        })();
        if let Err(e) = result {
            mouse.state.lock().unwrap().error = Some(e);
        } else {
            discover(mouse);
        }
    }
    impl Input {
        pub unsafe fn message(&mut self, msg: *const MSG, mouse: &SpaceMouse) {
            // Pointer lifetime is guaranteed by Winit's message-hook contract.
            let msg = unsafe { &*msg };
            if msg.message == WM_KILLFOCUS {
                mouse.state.lock().unwrap().clear();
                return;
            }
            if msg.message == WM_INPUT_DEVICE_CHANGE {
                self.devices.clear();
                discover(mouse);
                return;
            }
            if msg.message != WM_INPUT || msg.wParam != 0 {
                return;
            }
            let mut bytes = 0;
            let header_size = size_of::<RAWINPUTHEADER>() as u32;
            unsafe {
                if GetRawInputData(
                    msg.lParam as _,
                    RID_INPUT,
                    ptr::null_mut(),
                    &mut bytes,
                    header_size,
                ) == u32::MAX
                    || bytes > 65536
                    || bytes < header_size + 8
                {
                    return;
                }
                self.buffer.resize((bytes as usize).div_ceil(8), 0);
                let copied = GetRawInputData(
                    msg.lParam as _,
                    RID_INPUT,
                    self.buffer.as_mut_ptr().cast(),
                    &mut bytes,
                    header_size,
                );
                if copied == u32::MAX || copied < header_size + 8 {
                    return;
                }
                let header = ptr::read_unaligned(self.buffer.as_ptr().cast::<RAWINPUTHEADER>());
                if header.dwType != RIM_TYPEHID {
                    return;
                }
                let recognized = *self
                    .devices
                    .entry(header.hDevice as usize)
                    .or_insert_with(|| device(header.hDevice).is_some());
                if !recognized {
                    return;
                }
                let data =
                    std::slice::from_raw_parts(self.buffer.as_ptr().cast::<u8>(), copied as usize);
                let offset = offset_of!(RAWINPUT, data);
                let Some(hid) = data.get(offset..) else {
                    return;
                };
                if hid.len() < 8 {
                    return;
                }
                let size = u32::from_le_bytes(hid[..4].try_into().unwrap()) as usize;
                let count = u32::from_le_bytes(hid[4..8].try_into().unwrap()) as usize;
                if size == 0
                    || count > 1024
                    || size.checked_mul(count).is_none_or(|n| n > hid.len() - 8)
                {
                    return;
                }
                let mut s = mouse.state.lock().unwrap();
                let mut changed = false;
                for report in hid[8..8 + size * count].chunks_exact(size) {
                    changed |= s.report(report, Instant::now());
                }
                if changed && let Some(ctx) = &s.ctx {
                    ctx.request_repaint();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hid_axes_move_the_object_in_device_directions_at_every_view() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200., 800.));
        for direction in [Vec3::Z, Vec3::NEG_Y, Vec3::new(1., -1., 1.)] {
            for sign in [-1., 1.] {
                let mut base = Camera::default();
                base.set_view(direction);
                base.center = Vec3::new(20., 15., -9.);
                base.pivot = base.center + base.rotation * Vec3::new(4., -3., 8.);
                let landmark = base.center.to_array().map(f64::from);
                let before = base.project(landmark, rect);
                for (raw, expected) in [
                    (Vec3::X, egui::vec2(1., 0.)),
                    (Vec3::NEG_Z, egui::vec2(0., -1.)),
                ] {
                    let mut c = base.clone();
                    apply_motion(&mut c, raw * sign, Vec3::ZERO, 0.1, &Options::default());
                    let movement = c.project(landmark, rect) - before;
                    assert!(movement.dot(expected * sign) > 1.);
                }
                let mut c = base.clone();
                apply_motion(&mut c, Vec3::Y * sign, Vec3::ZERO, 0.1, &Options::default());
                assert!((c.span - base.span) * sign < 0.);
                for raw in [Vec3::X, Vec3::Y, Vec3::Z] {
                    let mut c = base.clone();
                    apply_motion(&mut c, Vec3::ZERO, raw * sign, 0.1, &Options::default());
                    let object_axis = Vec3::new(raw.x, -raw.z, raw.y) * sign;
                    let local_point = Vec3::new(9., 12., 17.);
                    let world_point = base.pivot + base.rotation * local_point;
                    let actual = c.rotation.conjugate() * (world_point - c.pivot);
                    let expected = Quat::from_scaled_axis(object_axis * 0.18) * local_point;
                    assert!((actual - expected).length() < 1e-4);
                    let pivot = c.pivot.to_array().map(f64::from);
                    assert!(c.project(pivot, rect).distance(base.project(pivot, rect)) < 0.002);
                }
            }
        }
    }
    fn report(id: u8, axes: &[i16]) -> Vec<u8> {
        std::iter::once(id)
            .chain(axes.iter().flat_map(|a| a.to_le_bytes()))
            .collect()
    }
    fn integrated_trace(frame_ms: u64, report_ms: u64) -> Vec3 {
        let start = Instant::now();
        let mut motion = Motion::default();
        let mut total = Vec3::ZERO;
        // Two level changes, a reversal, and a release, with independent USB
        // and frame schedules. Include a final frame after the filter settles.
        for ms in 0..=1200 {
            let now = start + Duration::from_millis(ms);
            if ms.is_multiple_of(report_ms) && ms <= 900 {
                let axes = match ms {
                    0..300 => [180, -70, 30],
                    300..600 => [300, 80, -150],
                    600..900 => [-180, 20, 100],
                    _ => [0; 3],
                };
                motion.report(axes, now);
            }
            if ms.is_multiple_of(frame_ms) || ms == 1200 {
                total += motion.take(now);
            }
        }
        assert!(!motion.active());
        total
    }
    #[test]
    fn filtered_motion_is_independent_of_redraw_and_report_cadence() {
        let baseline = integrated_trace(1, 10);
        for frames in [7, 16, 33, 50] {
            for reports in [5, 10, 20, 25, 50] {
                let actual = integrated_trace(frames, reports);
                assert!(
                    actual.distance(baseline) < 0.00001,
                    "{frames}ms frames / {reports}ms reports: {actual:?} vs {baseline:?}"
                );
            }
        }
    }
    #[test]
    fn filtering_reduces_sensor_steps_with_a_short_response_and_release() {
        let start = Instant::now();
        let mut motion = Motion::default();
        motion.report([350, 0, 0], start);
        motion.take(start + Duration::from_millis(12));
        // One time constant: responsive, but not a sudden full-speed jump.
        assert!((motion.velocity.x - (1. - (-1_f32).exp())).abs() < 0.00001);
        motion.take(start + Duration::from_millis(36));
        assert!(motion.velocity.x > 0.95);
        motion.report([0; 3], start + Duration::from_millis(40));
        motion.take(start + Duration::from_millis(40));
        let tail = motion.take(start + Duration::from_millis(160));
        assert!(tail.x > 0. && tail.x < RESPONSE_SECONDS);
        assert!(!motion.active());
        assert_eq!(motion.take(start + Duration::from_secs(30)), Vec3::ZERO);
    }
    #[test]
    fn alternating_sensor_noise_is_smoothed_without_queuing_old_reports() {
        let start = Instant::now();
        let mut motion = Motion::default();
        motion.report([200, 0, 0], start);
        motion.take(start + Duration::from_millis(100));
        let mut low = f32::MAX;
        let mut high = f32::MIN;
        for sample in 0..40 {
            let now = start + Duration::from_millis(100 + sample * 8);
            motion.report(
                [if sample.is_multiple_of(2) { 180 } else { 220 }, 0, 0],
                now,
            );
            motion.take(now);
            if sample > 10 {
                low = low.min(motion.velocity.x);
                high = high.max(motion.velocity.x);
            }
        }
        assert!(high - low < (axis(220) - axis(180)) * 0.5);
        // The latest value replaces the target; there is no queued sample tail.
        assert_eq!(motion.target.x, axis(220));
    }
    #[test]
    fn lost_reports_expire_and_focus_reset_discards_pending_motion() {
        let start = Instant::now();
        let mut motion = Motion::default();
        motion.report([350, 0, 0], start);
        let distance = motion.take(start + Duration::from_secs(1));
        // A lost release report cannot leave the camera moving indefinitely.
        assert!((distance.x - REPORT_TIMEOUT.as_secs_f32()).abs() < 0.00001);
        assert!(!motion.active());
        let mut state = State::default();
        state.report(&report(1, &[350, 50, -30, 150, 0, 10]), start);
        state.clear();
        let resumed = start + Duration::from_secs(2);
        assert_eq!(state.translation_motion.take(resumed), Vec3::ZERO);
        assert_eq!(state.rotation_motion.take(resumed), Vec3::ZERO);
        assert!(!state.translation_motion.active());
        assert!(!state.rotation_motion.active());
        state.report(&report(1, &[350, 0, 0]), resumed);
        assert_eq!(state.translation_motion.take(resumed), Vec3::ZERO);
        assert!(
            state
                .translation_motion
                .take(resumed + Duration::from_millis(16))
                .x
                > 0.
        );
    }
    #[test]
    fn split_axis_reports_keep_the_other_axes_moving_smoothly() {
        let start = Instant::now();
        let mut state = State::default();
        state.report(&report(1, &[350, 0, 0]), start);
        state.report(&report(2, &[0, 350, 0]), start + Duration::from_millis(8));
        let now = start + Duration::from_millis(24);
        let t = state.translation_motion.take(now);
        let r = state.rotation_motion.take(now);
        assert!(t.x > r.y && r.y > 0.);
        assert_eq!(t.y, 0.);
        assert_eq!(r.x, 0.);
        state.report(&report(1, &[0; 3]), now);
        state.report(&report(2, &[0; 3]), now);
        state
            .translation_motion
            .take(now + Duration::from_millis(120));
        state.rotation_motion.take(now + Duration::from_millis(120));
        assert!(!state.translation_motion.active());
        assert!(!state.rotation_motion.active());
    }
    #[test]
    fn split_combined_and_signed_hid_reports_decode_without_losing_axes() {
        let mut s = State::default();
        let now = Instant::now();
        assert!(s.report(&report(1, &[350, -123, 16]), now));
        assert!(s.report(&report(2, &[-35, 200, -350]), now));
        assert_eq!(
            s.translation_motion.target,
            Vec3::from_array([350, -123, 16].map(axis))
        );
        assert_eq!(
            s.rotation_motion.target,
            Vec3::from_array([-35, 200, -350].map(axis))
        );
        assert!(s.report(&report(1, &[35, 70, 105, 140, 175, 210]), now));
        assert_eq!(
            s.translation_motion.target,
            Vec3::from_array([35, 70, 105].map(axis))
        );
        assert_eq!(
            s.rotation_motion.target,
            Vec3::from_array([140, 175, 210].map(axis))
        );
        for bytes in [
            vec![],
            vec![1],
            vec![2, 1, 2],
            vec![3, 1],
            vec![9, 0, 0, 0, 0, 0, 0],
        ] {
            assert!(!s.report(&bytes, now));
        }
    }
    #[test]
    fn button_edges_deadzone_and_reset_are_stable() {
        let mut s = State::default();
        let now = Instant::now();
        s.report(&[3, 2, 0, 0, 0], now);
        assert_eq!(s.pressed, 2);
        s.pressed = 0;
        s.report(&[3, 2, 0, 0, 0], now);
        assert_eq!(s.pressed, 0);
        assert_eq!(axis(8), 0.);
        assert_eq!(axis(-8), 0.);
        assert_eq!(axis(350), 1.);
        assert_eq!(axis(-350), -1.);
        s.clear();
        assert!(!s.translation_motion.active());
        assert_eq!(s.buttons, 0);
    }
    #[test]
    fn six_axes_move_camera_and_motion_is_time_based() {
        for axis in 0..6 {
            let mut c = Camera::default();
            let old = c.clone();
            let mut t = Vec3::ZERO;
            let mut r = Vec3::ZERO;
            if axis < 3 {
                t[axis] = 0.5;
            } else {
                r[axis - 3] = 0.5;
            }
            apply_motion(&mut c, t, r, 0.016, &Options::default());
            assert!(
                (c.center - old.center).length() > 0.
                    || c.span != old.span
                    || c.rotation != old.rotation
            );
        }
        let mut a = Camera::default();
        let mut b = a.clone();
        apply_motion(&mut a, Vec3::Y, Vec3::ZERO, 0.032, &Options::default());
        for _ in 0..2 {
            apply_motion(&mut b, Vec3::Y, Vec3::ZERO, 0.016, &Options::default());
        }
        assert!((a.span - b.span).abs() < 1e-4);
        let mut a = Camera::default();
        let mut b = a.clone();
        apply_motion(
            &mut a,
            Vec3::new(0.5, 0.8, -0.3),
            Vec3::ZERO,
            0.1,
            &Options::default(),
        );
        for _ in 0..10 {
            apply_motion(
                &mut b,
                Vec3::new(0.5, 0.8, -0.3),
                Vec3::ZERO,
                0.01,
                &Options::default(),
            );
        }
        assert!((a.span - b.span).abs() < 1e-4);
        assert!(a.center.distance(b.center) < 1e-4);
    }
}
