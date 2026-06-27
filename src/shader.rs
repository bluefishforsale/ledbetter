//! GLSL runner: render a user fragment shader to an offscreen FBO at canvas
//! resolution, then read it back so Pixels still sample it on the CPU. The GPU
//! multiplier behind Book-of-Shaders / ShaderToy / MilkDrop-style content —
//! users paste shaders, so it's licensing-clean.
//!
//! Shader contract (desktop GL 3.3 core): write `void main()`, set `fragColor`,
//! using uniforms `u_resolution` (vec2) and `u_time` (float, = beats).

use std::sync::Arc;

use eframe::glow::{self, HasContext};

use crate::canvas::Canvas;

const VERT: &str = r#"#version 330 core
void main() {
    vec2 v[3] = vec2[3](vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    gl_Position = vec4(v[gl_VertexID], 0.0, 1.0);
}
"#;

const FRAG_HEADER: &str = r#"#version 330 core
out vec4 fragColor;
uniform vec2 u_resolution;
uniform float u_time;
uniform sampler2D u_feedback; // the previous frame, for MilkDrop-style feedback
"#;

/// A starter shader (original) so the runner shows something immediately.
pub const EXAMPLE: &str = r#"void main() {
    vec2 uv = gl_FragCoord.xy / u_resolution;
    float t = u_time;
    float v = sin(uv.x * 10.0 + t)
            + sin(uv.y * 10.0 + t * 1.3)
            + sin((uv.x + uv.y) * 10.0 - t);
    vec3 col = 0.5 + 0.5 * cos(6.2831 * (v * 0.2 + vec3(0.0, 0.33, 0.67)));
    fragColor = vec4(col, 1.0);
}
"#;

/// MilkDrop-style feedback (original): zoom+rotate the previous frame, decay it,
/// and add a moving spark. Paste into a deck's shader to see the tunnel/flow.
pub const FEEDBACK_EXAMPLE: &str = r#"void main() {
    vec2 uv = gl_FragCoord.xy / u_resolution;
    vec2 c = uv - 0.5;
    float a = 0.02;                       // rotate
    mat2 rot = mat2(cos(a), -sin(a), sin(a), cos(a));
    vec2 prev = rot * c * 0.98 + 0.5;     // zoom in slightly
    vec3 fb = texture(u_feedback, prev).rgb * 0.96;  // decay
    float d = length(c - 0.3 * vec2(cos(u_time), sin(u_time * 1.3)));
    vec3 spark = smoothstep(0.06, 0.0, d) * (0.5 + 0.5 * cos(u_time + vec3(0.0, 2.0, 4.0)));
    fragColor = vec4(max(fb, spark), 1.0);
}
"#;

pub struct GpuShader {
    gl: Arc<glow::Context>,
    program: glow::Program,
    fbo: glow::Framebuffer,
    tex: [glow::Texture; 2], // ping-pong: render into one while sampling the other
    write: usize,
    vao: glow::VertexArray,
    w: i32,
    h: i32,
    buf: Vec<u8>,
}

impl GpuShader {
    pub fn new(gl: Arc<glow::Context>, w: usize, h: usize, user_frag: &str) -> Result<Self, String> {
        unsafe {
            let program = link(&gl, VERT, &format!("{FRAG_HEADER}{user_frag}"))?;
            let fbo = gl.create_framebuffer()?;
            let tex = [gl.create_texture()?, gl.create_texture()?];
            for t in tex {
                gl.bind_texture(glow::TEXTURE_2D, Some(t));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    w as i32,
                    h as i32,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                // LINEAR + clamp so feedback warps sample smoothly.
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
                gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
                // clear to black so the first frame's feedback is empty
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(t),
                    0,
                );
                gl.clear_color(0.0, 0.0, 0.0, 1.0);
                gl.clear(glow::COLOR_BUFFER_BIT);
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            let vao = gl.create_vertex_array()?;
            Ok(GpuShader {
                gl,
                program,
                fbo,
                tex,
                write: 0,
                vao,
                w: w as i32,
                h: h as i32,
                buf: vec![0; w * h * 4],
            })
        }
    }

    /// Render the shader at time `t`, sampling the previous frame as u_feedback.
    pub fn render(&mut self, t: f32) {
        let gl = &self.gl;
        let read = self.tex[1 - self.write];
        let write = self.tex[self.write];
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(write),
                0,
            );
            gl.viewport(0, 0, self.w, self.h);
            gl.use_program(Some(self.program));
            let res = gl.get_uniform_location(self.program, "u_resolution");
            gl.uniform_2_f32(res.as_ref(), self.w as f32, self.h as f32);
            let time = gl.get_uniform_location(self.program, "u_time");
            gl.uniform_1_f32(time.as_ref(), t);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(read));
            let fb = gl.get_uniform_location(self.program, "u_feedback");
            gl.uniform_1_i32(fb.as_ref(), 0);
            gl.bind_vertex_array(Some(self.vao));
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
            gl.read_pixels(
                0,
                0,
                self.w,
                self.h,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut self.buf)),
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        self.write = 1 - self.write;
    }

    /// Copy the last render into a Canvas, flipping GL's bottom-up rows.
    pub fn copy_into(&self, canvas: &mut Canvas) {
        let (w, h) = (self.w as usize, self.h as usize);
        for y in 0..h.min(canvas.h) {
            let src = h - 1 - y;
            for x in 0..w.min(canvas.w) {
                let i = (src * w + x) * 4;
                canvas.set(x, y, [self.buf[i], self.buf[i + 1], self.buf[i + 2]]);
            }
        }
    }
}

impl Drop for GpuShader {
    fn drop(&mut self) {
        let gl = &self.gl;
        unsafe {
            gl.delete_program(self.program);
            gl.delete_framebuffer(self.fbo);
            gl.delete_texture(self.tex[0]);
            gl.delete_texture(self.tex[1]);
            gl.delete_vertex_array(self.vao);
        }
    }
}

fn link(gl: &glow::Context, vert: &str, frag: &str) -> Result<glow::Program, String> {
    unsafe {
        let program = gl.create_program()?;
        let mut shaders = Vec::new();
        for (kind, src) in [(glow::VERTEX_SHADER, vert), (glow::FRAGMENT_SHADER, frag)] {
            let s = gl.create_shader(kind)?;
            gl.shader_source(s, src);
            gl.compile_shader(s);
            if !gl.get_shader_compile_status(s) {
                let log = gl.get_shader_info_log(s);
                gl.delete_shader(s);
                for c in shaders {
                    gl.delete_shader(c);
                }
                gl.delete_program(program);
                return Err(log);
            }
            gl.attach_shader(program, s);
            shaders.push(s);
        }
        gl.link_program(program);
        let ok = gl.get_program_link_status(program);
        for c in shaders {
            gl.detach_shader(program, c);
            gl.delete_shader(c);
        }
        if !ok {
            let log = gl.get_program_info_log(program);
            gl.delete_program(program);
            return Err(log);
        }
        Ok(program)
    }
}
