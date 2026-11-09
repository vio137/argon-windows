/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Blur only the application's already rendered framebuffer under floating chrome.
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use glow::HasContext;

#[derive(Default)]
pub(super) struct Backdrop {
    effect: Option<Effect>,
    failed: bool,
}

struct Effect {
    program: glow::NativeProgram,
    texture: glow::NativeTexture,
    vao: glow::NativeVertexArray,
}

impl Backdrop {
    pub(super) fn add(state: &Arc<Mutex<Self>>, ctx: &egui::Context, id: egui::Id, order: egui::Order) {
        let Some(rect) = ctx.memory(|m| m.area_rect(id)) else {
            ctx.request_repaint();
            return;
        };
        let state = state.clone();
        ctx.layer_painter(egui::LayerId::new(order, id)).add(egui::PaintCallback {
            rect,
            callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
                if let Ok(mut state) = state.lock() {
                    state.paint(painter.gl(), info);
                }
            })),
        });
    }

    #[expect(unsafe_code)]
    fn paint(&mut self, gl: &glow::Context, info: egui::PaintCallbackInfo) {
        if self.failed { return; }
        if self.effect.is_none() {
            match unsafe { Effect::new(gl) } {
                Ok(effect) => self.effect = Some(effect),
                Err(error) => {
                    log::warn!("In-app backdrop blur unavailable: {error}");
                    self.failed = true;
                    return;
                }
            }
        }
        let Some(effect) = &self.effect else { return; };
        let [width, height] = info.screen_size_px;
        if width == 0 || height == 0 { return; }
        unsafe {
            // Servo's page blit leaves READ bound to its own offscreen surface.
            // Capture DRAW (the app frame), never a desktop or another window.
            let old_read = gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
            let draw = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, NonZeroU32::new(draw as u32).map(glow::NativeFramebuffer));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(effect.texture));
            gl.copy_tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA, 0, 0, width as i32, height as i32, 0);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, NonZeroU32::new(old_read as u32).map(glow::NativeFramebuffer));
            gl.use_program(Some(effect.program));
            gl.bind_vertex_array(Some(effect.vao));
            gl.uniform_1_i32(gl.get_uniform_location(effect.program, "source").as_ref(), 0);
            gl.uniform_2_f32(gl.get_uniform_location(effect.program, "screen").as_ref(), width as f32, height as f32);
            let viewport = info.viewport_in_pixels();
            gl.uniform_4_f32(gl.get_uniform_location(effect.program, "region").as_ref(), viewport.left_px as f32, viewport.from_bottom_px as f32, viewport.width_px as f32, viewport.height_px as f32);
            gl.disable(glow::BLEND);
            gl.draw_arrays(glow::TRIANGLES, 0, 3);
        }
        // egui_glow restores all painting state after a callback.
    }

    #[expect(unsafe_code)]
    pub(super) fn destroy(&mut self, gl: &glow::Context) {
        if let Some(effect) = self.effect.take() {
            unsafe {
                gl.delete_program(effect.program);
                gl.delete_texture(effect.texture);
                gl.delete_vertex_array(effect.vao);
            }
        }
    }
}

impl Effect {
    #[expect(unsafe_code)]
    unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let version = if gl.get_parameter_string(glow::VERSION).contains("OpenGL ES") {
                "#version 300 es\nprecision highp float;\n"
            } else { "#version 150\n" };
            let program = gl.create_program()?;
            let mut shaders = Vec::new();
            let sources = [
                (glow::VERTEX_SHADER, "void main(){vec2 p=vec2(float((gl_VertexID<<1)&2),float(gl_VertexID&2));gl_Position=vec4(p*2.0-1.0,0.0,1.0);}"),
                (glow::FRAGMENT_SHADER, "uniform sampler2D source;uniform vec2 screen;uniform vec4 region;out vec4 color;void main(){vec2 q=gl_FragCoord.xy-region.xy;vec2 d=abs(q-region.zw*0.5)-(region.zw*0.5-vec2(14.0));if(length(max(d,0.0))+min(max(d.x,d.y),0.0)>14.0)discard;vec4 sum=vec4(0.0);float total=0.0;for(int y=-2;y<=2;y++){for(int x=-2;x<=2;x++){vec2 delta=vec2(float(x),float(y));float weight=exp(-dot(delta,delta)*0.35);sum+=texture(source,(gl_FragCoord.xy+delta*5.0)/screen)*weight;total+=weight;}}color=sum/total;}"),
            ];
            for (kind, source) in sources {
                let shader = gl.create_shader(kind)?;
                gl.shader_source(shader, &format!("{version}{source}"));
                gl.compile_shader(shader);
                if !gl.get_shader_compile_status(shader) {
                    let message = gl.get_shader_info_log(shader);
                    gl.delete_shader(shader);
                    for old in shaders { gl.delete_shader(old); }
                    gl.delete_program(program);
                    return Err(message);
                }
                gl.attach_shader(program, shader);
                shaders.push(shader);
            }
            gl.link_program(program);
            for shader in shaders { gl.detach_shader(program, shader); gl.delete_shader(shader); }
            if !gl.get_program_link_status(program) {
                let message = gl.get_program_info_log(program);
                gl.delete_program(program);
                return Err(message);
            }
            let texture = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
            let vao = gl.create_vertex_array()?;
            Ok(Self { program, texture, vao })
        }
    }
}
