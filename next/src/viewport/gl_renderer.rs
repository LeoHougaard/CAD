//! Depth-buffered display triangles. Modeling and native element identities stay outside GL.
use eframe::{egui, egui_glow, glow};
use glow::HasContext as _;
use std::sync::{Arc, Mutex};

struct Resources {
    program: glow::Program,
    vao: glow::VertexArray,
    buffer: glow::Buffer,
}

pub(super) struct Renderer {
    gl: Arc<glow::Context>,
    resources: Mutex<Option<Resources>>,
}

impl Renderer {
    pub(super) fn new(gl: Arc<glow::Context>) -> Result<Arc<Self>, String> {
        let version = if gl.version().is_embedded {
            "#version 300 es\nprecision highp float;\n"
        } else {
            "#version 330 core\n"
        };
        let vertex = format!(
            "{version}layout(location=0) in vec3 position;\nlayout(location=1) in vec3 color;\nout vec3 vertex_color;\nvoid main() {{ gl_Position=vec4(position,1.0); vertex_color=color; }}"
        );
        let fragment = format!(
            "{version}in vec3 vertex_color;\nout vec4 output_color;\nvoid main() {{ output_color=vec4(vertex_color,1.0); }}"
        );
        // The context is current during eframe creation and shutdown.
        let resources = unsafe {
            let program = gl.create_program()?;
            let vs = match compile(&gl, glow::VERTEX_SHADER, &vertex) {
                Ok(shader) => shader,
                Err(error) => {
                    gl.delete_program(program);
                    return Err(error);
                }
            };
            let fs = match compile(&gl, glow::FRAGMENT_SHADER, &fragment) {
                Ok(shader) => shader,
                Err(error) => {
                    gl.delete_shader(vs);
                    gl.delete_program(program);
                    return Err(error);
                }
            };
            gl.attach_shader(program, vs);
            gl.attach_shader(program, fs);
            gl.link_program(program);
            gl.detach_shader(program, vs);
            gl.detach_shader(program, fs);
            gl.delete_shader(vs);
            gl.delete_shader(fs);
            if !gl.get_program_link_status(program) {
                let error = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(error);
            }
            let vao = match gl.create_vertex_array() {
                Ok(vao) => vao,
                Err(error) => {
                    gl.delete_program(program);
                    return Err(error);
                }
            };
            let buffer = match gl.create_buffer() {
                Ok(buffer) => buffer,
                Err(error) => {
                    gl.delete_vertex_array(vao);
                    gl.delete_program(program);
                    return Err(error);
                }
            };
            gl.bind_vertex_array(Some(vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(buffer));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, 24, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 24, 12);
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            Resources {
                program,
                vao,
                buffer,
            }
        };
        Ok(Arc::new(Self {
            gl,
            resources: Mutex::new(Some(resources)),
        }))
    }

    pub(super) fn callback(
        self: &Arc<Self>,
        rect: egui::Rect,
        vertices: Vec<f32>,
    ) -> egui::PaintCallback {
        let renderer = Arc::clone(self);
        egui::PaintCallback {
            rect,
            callback: Arc::new(egui_glow::CallbackFn::new(move |_info, _painter| {
                renderer.paint(&vertices);
            })),
        }
    }

    fn paint(&self, vertices: &[f32]) {
        let Ok(resources) = self.resources.lock() else {
            return;
        };
        let Some(resources) = resources.as_ref() else {
            return;
        };
        // egui_glow has already set the viewport and scissor to this callback.
        // It restores its program, VAO, buffers, blending, and viewport afterward.
        unsafe {
            let gl = &self.gl;
            gl.enable(glow::DEPTH_TEST);
            gl.depth_mask(true);
            gl.depth_func(glow::LEQUAL);
            gl.clear_depth_f32(1.0);
            gl.clear(glow::DEPTH_BUFFER_BIT);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::BLEND);
            gl.use_program(Some(resources.program));
            gl.bind_vertex_array(Some(resources.vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(resources.buffer));
            gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(vertices),
                glow::STREAM_DRAW,
            );
            gl.draw_arrays(glow::TRIANGLES, 0, (vertices.len() / 6) as i32);
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.disable(glow::DEPTH_TEST);
        }
    }

    /// Must run from App::on_exit while the renderer's GL context is current.
    pub(super) fn destroy(&self) {
        let Ok(mut resources) = self.resources.lock() else {
            return;
        };
        if let Some(resources) = resources.take() {
            unsafe {
                self.gl.delete_buffer(resources.buffer);
                self.gl.delete_vertex_array(resources.vao);
                self.gl.delete_program(resources.program);
            }
        }
    }
}

unsafe fn compile(gl: &glow::Context, kind: u32, source: &str) -> Result<glow::Shader, String> {
    unsafe {
        let shader = gl.create_shader(kind)?;
        gl.shader_source(shader, source);
        gl.compile_shader(shader);
        if gl.get_shader_compile_status(shader) {
            Ok(shader)
        } else {
            let error = gl.get_shader_info_log(shader);
            gl.delete_shader(shader);
            Err(error)
        }
    }
}
