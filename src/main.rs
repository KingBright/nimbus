use futures_util::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;
use winit::{
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

const PET_WINDOW_WIDTH_RATIO: f64 = 0.88;
const PET_WINDOW_HEIGHT_RATIO: f64 = 0.50;
const NIMBUS_WINDOW_HEIGHT_RATIO: f64 = 0.54;
const BUBBLE_INITIAL_WIDTH: u32 = 100;
const BUBBLE_INITIAL_HEIGHT: u32 = 64;

mod activity;
mod art;
mod behavior;
mod config;
mod desktop;
mod placement;
mod storage;
mod llama_engine;
mod memory;
mod renderer;
mod state;
mod think_filter;

use activity::observe_user_activity;
use behavior::{
    build_behavior_prompt, clean_behavior_line, jittered_interval, BehaviorContext, BehaviorEvent,
};
use config::AppConfig;
use llama_engine::LlamaEngine;
use memory::PetMemory;
use renderer::PetRenderer;
use state::{decide_state, PetState, StateContext, StateMachine};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    TrayIconBuilder,
};

struct WgpuState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    render_pipeline: wgpu::RenderPipeline,
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

impl WgpuState {
    async fn new(
        window: Arc<winit::window::Window>,
        _width: u32,
        _height: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or("Failed to find an appropriate adapter")?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let alpha_mode = if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else {
            wgpu::CompositeAlphaMode::Opaque
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let texture_desc = wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("Skia Texture"),
            view_formats: &[],
        };
        let texture = device.create_texture(&texture_desc);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
                r#"
                struct VertexOutput {
                    @builtin(position) clip_position: vec4<f32>,
                    @location(0) tex_coords: vec2<f32>,
                }

                @vertex
                fn vs_main(
                    @builtin(vertex_index) in_vertex_index: u32,
                ) -> VertexOutput {
                    var pos = array<vec2<f32>, 6>(
                        vec2<f32>(-1.0,  1.0),
                        vec2<f32>(-1.0, -1.0),
                        vec2<f32>( 1.0, -1.0),
                        vec2<f32>(-1.0,  1.0),
                        vec2<f32>( 1.0, -1.0),
                        vec2<f32>( 1.0,  1.0)
                    );
                    var uv = array<vec2<f32>, 6>(
                        vec2<f32>(0.0, 0.0),
                        vec2<f32>(0.0, 1.0),
                        vec2<f32>(1.0, 1.0),
                        vec2<f32>(0.0, 0.0),
                        vec2<f32>(1.0, 1.0),
                        vec2<f32>(1.0, 0.0)
                    );

                    var out: VertexOutput;
                    out.clip_position = vec4<f32>(pos[in_vertex_index], 0.0, 1.0);
                    out.tex_coords = uv[in_vertex_index];
                    return out;
                }

                @group(0) @binding(0) var t_diffuse: texture_2d<f32>;
                @group(0) @binding(1) var s_diffuse: sampler;

                @fragment
                fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
                    return textureSample(t_diffuse, s_diffuse, in.tex_coords);
                }
            "#,
            )),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
            label: Some("texture_bind_group_layout"),
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("texture_bind_group"),
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            render_pipeline,
            texture,
            texture_view,
            sampler,
            bind_group,
            width,
            height,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 && (self.width != width || self.height != height) {
            self.width = width;
            self.height = height;
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);

            let texture_desc = wgpu::TextureDescriptor {
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                label: Some("Skia Texture"),
                view_formats: &[],
            };
            self.texture = self.device.create_texture(&texture_desc);
            self.texture_view = self
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            let bind_group_layout =
                self.device
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        entries: &[
                            wgpu::BindGroupLayoutEntry {
                                binding: 0,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Texture {
                                    multisampled: false,
                                    view_dimension: wgpu::TextureViewDimension::D2,
                                    sample_type: wgpu::TextureSampleType::Float {
                                        filterable: true,
                                    },
                                },
                                count: None,
                            },
                            wgpu::BindGroupLayoutEntry {
                                binding: 1,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                                count: None,
                            },
                        ],
                        label: Some("texture_bind_group_layout"),
                    });

            self.bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
                label: Some("texture_bind_group"),
            });
        }
    }

    fn render(&self, skia_data: &[u8]) -> Result<(), wgpu::SurfaceError> {
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            skia_data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

fn sync_renderer_to_window(
    renderer: &mut PetRenderer,
    window: &winit::window::Window,
    font_size: f32,
) -> winit::dpi::PhysicalSize<u32> {
    let size = window.inner_size();
    renderer.scale_factor = window.scale_factor() as f32;
    renderer.resize(size.width.max(1), size.height.max(1));
    renderer.set_font_size(font_size);
    size
}

fn pet_window_logical_size(nominal_size: u32, pet_style: &str) -> (u32, u32) {
    let width = (nominal_size as f64 * PET_WINDOW_WIDTH_RATIO)
        .round()
        .max(1.0) as u32;
    let height = if pet_style == "nimbus" {
        (nominal_size as f64 * NIMBUS_WINDOW_HEIGHT_RATIO)
            .round()
            .max(1.0) as u32
    } else {
        (nominal_size as f64 * PET_WINDOW_HEIGHT_RATIO)
            .round()
            .max(1.0) as u32
    };
    (width, height)
}

fn resize_pet_window(
    window: &winit::window::Window,
    wgpu_state: &mut WgpuState,
    renderer: &mut PetRenderer,
    nominal_size: u32,
    pet_style: &str,
    font_size: f32,
) {
    let (logical_width, logical_height) = pet_window_logical_size(nominal_size, pet_style);
    let scale = window.scale_factor();
    let old_size = window.inner_size();
    let old_position = window.outer_position().ok();
    let fallback_size = winit::dpi::PhysicalSize::new(
        (logical_width as f64 * scale).round().max(1.0) as u32,
        (logical_height as f64 * scale).round().max(1.0) as u32,
    );
    let target_size = window
        .request_inner_size(winit::dpi::LogicalSize::new(
            logical_width as f64,
            logical_height as f64,
        ))
        .unwrap_or(fallback_size);

    if let Some(old_position) = old_position {
        let x = old_position.x + (old_size.width as i32 - target_size.width as i32) / 2;
        let y = old_position.y + old_size.height as i32 - target_size.height as i32;
        window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
    }

    wgpu_state.resize(target_size.width.max(1), target_size.height.max(1));
    renderer.scale_factor = scale as f32;
    renderer.resize(target_size.width.max(1), target_size.height.max(1));
    renderer.set_font_size(font_size);
}

fn set_pet_pointer_from_physical(
    renderer: &mut PetRenderer,
    window_size: winit::dpi::PhysicalSize<u32>,
    x: f64,
    y: f64,
) {
    renderer.set_mouse_position(Some((x as f32, y as f32)));
    renderer.pointer_x = x as f32 / (window_size.width.max(1) as f32 / 24.0);
    renderer.pointer_y = y as f32 / (window_size.height.max(1) as f32 / 24.0);
}

// Pseudo-random index selector without rand dependency
fn get_pseudo_random_index(max: usize) -> usize {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    (now % max as u128) as usize
}

struct StyleSpeechProfile {
    startup_line: &'static str,
    switch_line: &'static str,
    touch_note: &'static str,
    touch_lines: &'static [&'static str],
    sleep_line: &'static str,
    work_line: &'static str,
    browser_line: &'static str,
    chat_line: &'static str,
    generic_lines: &'static [&'static str],
}

const NIMBUS_TOUCH_LINES: &[&str] = &[
    "哎呀，摸到我啦~",
    "软软地蹭一下你。",
    "小云朵被摸得轻轻晃了一下。",
];
const NIMBUS_GENERIC_LINES: &[&str] = &[
    "云朵今天也想飞得更高呢~",
    "我在空气里轻轻漂着。",
    "软软打个滚，像一小团棉花糖。",
];

const MOCHI_TOUCH_LINES: &[&str] = &[
    "喵呜，摸到耳朵边啦~",
    "我轻轻甩了甩尾巴。",
    "爪爪收起来，乖乖让你摸。",
];
const MOCHI_GENERIC_LINES: &[&str] = &[
    "我舔舔爪子，安静陪你。",
    "尾巴轻轻摇了一下。",
    "喵呜，今天也想靠近一点。",
];

const STARLIT_TOUCH_LINES: &[&str] = &[
    "咕噜噜，触角轻轻卷起来。",
    "星光皇冠晃了一下。",
    "我吐出一颗小小星泡。",
];
const STARLIT_GENERIC_LINES: &[&str] = &[
    "触角慢慢摆动，星光闪了一下。",
    "咕噜噜，我在屏幕边漂浮。",
    "一颗小星泡轻轻升起来。",
];

const CRAB_TOUCH_LINES: &[&str] = &[
    "咔嚓，摸到我的小壳啦~",
    "大螯轻轻收起来，不夹你。",
    "小螃蟹吐了个泡泡。",
];
const CRAB_GENERIC_LINES: &[&str] = &[
    "我横着走两步，悄悄举起大螯。",
    "咔嚓咔嚓，今天也精神满满。",
    "小泡泡从蟹壳边冒出来。",
];

const LUMA_TOUCH_LINES: &[&str] = &[
    "长耳朵轻轻靠过来，碰到你啦。",
    "圆尾巴晃了一下，我又挨近一点。",
    "月牙光纹被摸得亮了一小圈。",
];
const LUMA_GENERIC_LINES: &[&str] = &[
    "我抖抖长耳朵，接住一小片月光。",
    "圆尾巴轻轻一晃，我又蹦近了一点。",
    "胸前的月牙光纹悄悄亮起来。",
];

fn style_speech_profile(pet_style: &str) -> StyleSpeechProfile {
    match pet_style {
        "mochi" => StyleSpeechProfile {
            startup_line: "我醒啦，竖起耳朵，今天也在这里陪你。",
            switch_line: "耳朵轻轻竖起来，我正在熟悉这副小猫身体。",
            touch_note: "我会记住对方触碰猫耳和爪爪时更喜欢柔和、轻一点的反应。",
            touch_lines: MOCHI_TOUCH_LINES,
            sleep_line: "夜深了，小猫把尾巴圈起来，安静陪着你。",
            work_line: "我收起爪爪，趴在一旁陪你写代码。",
            browser_line: "我眨眨眼，提醒你让眼睛歇一下。",
            chat_line: "我轻轻摇尾巴，等你聊完。",
            generic_lines: MOCHI_GENERIC_LINES,
        },
        "starlit" => StyleSpeechProfile {
            startup_line: "我醒啦，舒展触角，今天也在这里陪你。",
            switch_line: "触角慢慢舒展开，我正在熟悉这副星光身体。",
            touch_note: "我会记住对方触碰触角时更喜欢柔和、轻一点的反应。",
            touch_lines: STARLIT_TOUCH_LINES,
            sleep_line: "夜深了，星光章鱼抱住小星泡，安静陪着你。",
            work_line: "触角轻轻摆动，我在一旁陪你写代码。",
            browser_line: "我吐出一颗星泡，提醒你让眼睛歇一下。",
            chat_line: "触角慢慢晃着，我安静等你聊完。",
            generic_lines: STARLIT_GENERIC_LINES,
        },
        "crab" => StyleSpeechProfile {
            startup_line: "我醒啦，活动一下大螯，今天也在这里陪你。",
            switch_line: "大螯咔嚓轻响，我正在熟悉这副小螃蟹身体。",
            touch_note: "我会记住对方触碰蟹壳和大螯时更喜欢柔和、轻一点的反应。",
            touch_lines: CRAB_TOUCH_LINES,
            sleep_line: "夜深了，小螃蟹把大螯收近一点，安静陪着你。",
            work_line: "咔嚓轻响，我横着挪到一旁陪你写代码。",
            browser_line: "我吐出一个小泡泡，提醒你让眼睛歇一下。",
            chat_line: "小螃蟹晃晃大螯，安静等你聊完。",
            generic_lines: CRAB_GENERIC_LINES,
        },
        "luma" => StyleSpeechProfile {
            startup_line: "我醒啦，抖抖长耳朵，今天也在这里陪你。",
            switch_line: "长耳朵慢慢竖起来，我正在熟悉这副月灯兔身体。",
            touch_note: "我会记住对方触碰长耳朵和月牙光纹时更喜欢柔和、轻一点的反应。",
            touch_lines: LUMA_TOUCH_LINES,
            sleep_line: "夜深了，我把长耳朵放松下来，抱着一点月光陪你。",
            work_line: "我收好圆尾巴，安静蹲在一旁陪你写代码。",
            browser_line: "我抖抖长耳朵，提醒你让眼睛歇一下。",
            chat_line: "月牙光纹轻轻亮着，我安静等你聊完。",
            generic_lines: LUMA_GENERIC_LINES,
        },
        _ => StyleSpeechProfile {
            startup_line: "我醒啦，轻轻漂过来，今天也在这里陪你。",
            switch_line: "身体轻轻漂起来，我正在熟悉这副云朵身体。",
            touch_note: "我会记住对方触碰云朵身体时更喜欢柔和、轻一点的反应。",
            touch_lines: NIMBUS_TOUCH_LINES,
            sleep_line: "夜深了，小云朵收起光边，安静陪着你。",
            work_line: "我轻轻漂在一旁，陪你写代码。",
            browser_line: "我吹来一点小风，提醒你让眼睛歇一下。",
            chat_line: "我在旁边轻轻漂着，等你聊完。",
            generic_lines: NIMBUS_GENERIC_LINES,
        },
    }
}

fn startup_fallback_line(pet_style: &str) -> &'static str {
    style_speech_profile(pet_style).startup_line
}

fn contains_legacy_pet_name(text: &str) -> bool {
    text.to_lowercase().contains("nimbus") || text.contains("灵霓")
}

fn pet_name_prompt_suffix(pet_name: &str) -> String {
    let pet_name = pet_name.trim();
    if pet_name.is_empty() || contains_legacy_pet_name(pet_name) {
        String::new()
    } else {
        format!("，名字叫{pet_name}")
    }
}
fn style_switch_line(pet_style: &str) -> &'static str {
    style_speech_profile(pet_style).switch_line
}

fn touch_response_lines(pet_style: &str) -> &'static [&'static str] {
    style_speech_profile(pet_style).touch_lines
}

fn touch_memory_note(pet_style: &str) -> &'static str {
    style_speech_profile(pet_style).touch_note
}

fn weather_feedback_line(pet_style: &str, weather_mode: &str) -> &'static str {
    match (pet_style, weather_mode) {
        ("crab", "sunny") => "阳光照在蟹壳上，大螯亮了一下。",
        ("crab", "windy") => "风有点大，我把大螯贴近身体。",
        ("crab", "rainy") => "雨声轻轻落下，我吐出一串小泡泡。",
        ("crab", "snowy") => "雪花落下来，我把小脚缩近一点。",
        ("mochi", "sunny") => "阳光暖暖的，我把尾巴晒得软软的。",
        ("mochi", "windy") => "风吹乱了耳朵，我轻轻甩甩头。",
        ("mochi", "rainy") => "雨声细细的，我把爪爪收起来。",
        ("mochi", "snowy") => "雪花落下来，我把尾巴圈紧一点。",
        ("starlit", "sunny") => "阳光穿过星泡，触角亮了一下。",
        ("starlit", "windy") => "风把星泡吹远了，我慢慢漂回来。",
        ("starlit", "rainy") => "雨点敲着星泡，咕噜噜地响。",
        ("starlit", "snowy") => "雪花绕着皇冠转了一圈。",
        ("luma", "sunny") => "暖光落在长耳朵上，月牙纹也亮了一点。",
        ("luma", "windy") => "风从耳尖掠过，我轻轻抖了抖耳朵。",
        ("luma", "rainy") => "雨声落下来，我把圆尾巴收近一点。",
        ("luma", "snowy") => "雪花停在耳尖，我捧住一小点冷光。",
        (_, "windy") => "风吹过来，我轻轻晃了一下。",
        (_, "rainy") => "雨声细细落下，我安静陪着你。",
        (_, "snowy") => "雪花慢慢飘下来了，空气变得很轻。",
        _ => "晴光落下来，心情也亮了一点。",
    }
}

// Offline muttering template based on active app and local time.
// This is only used when the model cannot answer, so it must stay style-aware.
fn get_offline_mutter(app: &str, hour: u32, pet_style: &str) -> String {
    let profile = style_speech_profile(pet_style);
    if hour >= 23 || hour <= 5 {
        return profile.sleep_line.to_string();
    }

    let app_lower = app.to_lowercase();
    if app_lower.contains("code")
        || app_lower.contains("terminal")
        || app_lower.contains("xcode")
        || app_lower.contains("cargo")
        || app_lower.contains("idea")
    {
        return profile.work_line.to_string();
    }
    if app_lower.contains("safari")
        || app_lower.contains("chrome")
        || app_lower.contains("firefox")
        || app_lower.contains("browser")
    {
        return profile.browser_line.to_string();
    }
    if app_lower.contains("chat")
        || app_lower.contains("wechat")
        || app_lower.contains("slack")
        || app_lower.contains("discord")
        || app_lower.contains("ding")
    {
        return profile.chat_line.to_string();
    }

    let idx = get_pseudo_random_index(profile.generic_lines.len());
    profile.generic_lines[idx].to_string()
}

fn create_tray_icon() -> tray_icon::Icon {
    use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};
    let mut pixmap = Pixmap::new(18, 18).unwrap();
    let mut path = PathBuilder::new();
    path.move_to(7.0, 24.0);
    path.cubic_to(0.5, 24.0, 0.5, 14.5, 6.5, 13.0);
    path.cubic_to(5.0, 6.0, 13.5, 3.5, 17.5, 8.0);
    path.cubic_to(24.0, 3.5, 29.0, 8.0, 27.0, 14.0);
    path.cubic_to(33.5, 16.5, 31.0, 24.0, 25.0, 24.0);
    path.close();
    let mut paint = Paint::default();
    paint.set_color_rgba8(0, 0, 0, 255);
    paint.anti_alias = true;
    let transform = Transform::from_scale(18.0 / 32.0, 18.0 / 32.0);
    pixmap.fill_path(&path.finish().unwrap(), &paint, FillRule::Winding, transform, None);
    let mut eyes = PathBuilder::new();
    eyes.push_circle(12.0, 16.0, 1.35);
    eyes.push_circle(21.0, 16.0, 1.35);
    paint.blend_mode = tiny_skia::BlendMode::Clear;
    pixmap.fill_path(&eyes.finish().unwrap(), &paint, FillRule::Winding, transform, None);
    tray_icon::Icon::from_rgba(pixmap.data().to_vec(), 18, 18).unwrap()
}

fn get_persona_prompt(
    persona: &str,
    pet_style: &str,
    pet_name: &str,
    weather_mode: &str,
) -> String {
    let style_desc = match pet_style {
        "mochi" => "你当前的外观是一只软萌的抹茶小猫，耳朵尖尖，适合用喵呜、舔爪、摇尾巴这类身体经验来回应。".to_string(),
        "starlit" => "你当前的外观是一只梦幻的紫色太空章鱼，头上戴着星光皇冠，适合用触角、星泡、漂浮和闪烁星光这类身体经验来回应。".to_string(),
        "crab" => "你当前的外观是一只神气活现的红色小螃蟹，长着一对大螯钳，适合用咔嚓轻响、挥动大螯、横着走路、吐泡泡这类身体经验来回应；如果旧记忆里出现其他旧外观称呼，把它理解为过去造型，不主动沿用。".to_string(),
        "luma" => "你当前的外观是一只柔软灵动的月灯兔，长耳朵、圆尾巴和胸前的月牙光纹都属于你，适合用抖耳朵、轻轻蹦跳、靠近指尖和捧住月光这类身体经验来回应；如果旧记忆里出现其他旧外观称呼，把它理解为过去造型，不主动沿用。".to_string(),
        "nimbus" => "你当前的外观是一只雪白蓬松的云朵精灵，像一团棉花糖一样在空中漂浮，适合用轻飘飘、软绵绵变形、小雨滴或闪电这类身体经验来回应。".to_string(),
        _ => "你当前的外观是一只可爱的桌宠。".to_string(),
    };
    let name_suffix = pet_name_prompt_suffix(pet_name);

    let base_desc = match persona {
        "tsundere" => format!("你是一只傲娇性格的桌宠{name_suffix}。说话可以带一点'哼'和嘴硬感，关心对方但不直接承认，语气别扭但有温度。"),
        "toxic" => format!("你是一只毒舌性格的桌宠{name_suffix}。喜欢一针见血地吐槽日常，语气像冷幽默而不是攻击，内心仍然在陪伴对方。"),
        "moe" => format!("你是一只软萌粘人、充满治愈感的桌宠{name_suffix}。说话甜而轻，多用柔软的小动作表达依赖和陪伴。"),
        _ => format!("你是一只可爱的桌宠{name_suffix}。性格软萌活泼，说话温柔体贴，充满好奇心和活力。"),
    };

    let weather_desc = match weather_mode {
        "sunny" => "☀️晴朗舒适，阳光明媚，周围带着温暖的光晕，让人感到温暖和惬意。",
        "windy" => "💨狂风大作，气流卷动，周围刮着凉风，你可能会在风中摇曳或凌乱。",
        "rainy" => "🌧️细雨绵绵，雨滴轻轻滑落，空气潮湿而温柔。",
        "snowy" => "❄️晶莹剔透的雪花正缓缓旋转飘落，天寒地冻，适合把动作放轻一点。",
        _ => "☀️晴天",
    };

    format!("{} {} 当前环境天气是：[{}]。请结合当前外观、性格和天气，直接说一句自然中文台词，优先写具体身体动作、观察或情绪，避免解释设定或提到提示词。", base_desc, style_desc, weather_desc)
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GenerationKind {
    Chat,
    Behavior(BehaviorEvent),
}

struct ActiveGeneration {
    kind: GenerationKind,
    context: Option<BehaviorContext>,
    retry_count: u8,
    started_at: std::time::Instant,
}

struct PendingBehavior {
    event: BehaviorEvent,
    context: BehaviorContext,
}

struct TextReveal {
    full_text: Vec<char>,
    visible_chars: usize,
    next_step: std::time::Instant,
    step: std::time::Duration,
    display_duration: std::time::Duration,
}

struct TrayControls {
    mutter_enabled: CheckMenuItem,
    mutter_intervals: [CheckMenuItem; 3],
    touch_cooldowns: [CheckMenuItem; 3],
    dream_intervals: [CheckMenuItem; 3],
    personas: [CheckMenuItem; 4],
    styles: [CheckMenuItem; 5],
    weather_modes: [CheckMenuItem; 4],
    font_sizes: [CheckMenuItem; 4],
    pet_sizes: [CheckMenuItem; 5],
    offload_timeouts: [CheckMenuItem; 3],
}

impl TrayControls {
    fn sync(&self, config: &AppConfig) {
        self.mutter_enabled.set_checked(config.mutter_enabled);
        for (item, value) in self.mutter_intervals.iter().zip([60, 180, 300]) {
            item.set_checked(config.mutter_interval_secs == value);
        }
        for (item, value) in self.touch_cooldowns.iter().zip([5, 15, 30]) {
            item.set_checked(config.touch_response_cooldown_secs == value);
        }
        for (item, value) in self.dream_intervals.iter().zip([45, 75, 120]) {
            item.set_checked(config.dream_interval_secs == value);
        }
        for (item, value) in self
            .personas
            .iter()
            .zip(["default", "tsundere", "toxic", "moe"])
        {
            item.set_checked(config.persona == value);
        }
        for (item, value) in self
            .styles
            .iter()
            .zip(["nimbus", "mochi", "starlit", "crab", "luma"])
        {
            item.set_checked(config.pet_style == value);
        }
        for (item, value) in self
            .weather_modes
            .iter()
            .zip(["sunny", "windy", "rainy", "snowy"])
        {
            item.set_checked(config.weather_mode == value);
        }
        for (item, value) in self.font_sizes.iter().zip([20.0, 24.0, 28.0, 32.0]) {
            item.set_checked((config.font_size - value).abs() < f32::EPSILON);
        }
        for (item, value) in self.pet_sizes.iter().zip([150, 200, 250, 300, 350]) {
            item.set_checked(config.pet_size == value);
        }
        for (item, value) in self.offload_timeouts.iter().zip([30, 60, 120]) {
            item.set_checked(config.idle_offload_secs == value);
        }
    }
}

impl TextReveal {
    fn new(text: String, now: std::time::Instant, display_duration: std::time::Duration) -> Self {
        Self {
            full_text: text.chars().collect(),
            visible_chars: 0,
            next_step: now,
            step: std::time::Duration::from_millis(48),
            display_duration,
        }
    }

    fn advance(&mut self, now: std::time::Instant) -> Option<String> {
        if now < self.next_step || self.visible_chars >= self.full_text.len() {
            return None;
        }
        self.visible_chars += 1;
        self.next_step = now + self.step;
        Some(self.full_text[..self.visible_chars].iter().collect())
    }

    fn is_complete(&self) -> bool {
        self.visible_chars >= self.full_text.len()
    }
}

fn behavior_context(
    config: &AppConfig,
    memory: &Arc<std::sync::Mutex<PetMemory>>,
    app: String,
    hour: u32,
) -> BehaviorContext {
    let (memory_summary, recent_lines) = memory
        .lock()
        .map(|mem| (mem.describe_for_prompt(), mem.recent_lines(8)))
        .unwrap_or_else(|_| ("还在了解屏幕前的人的节奏。".to_string(), Vec::new()));
    BehaviorContext {
        pet_style: config.pet_style.clone(),
        persona: config.persona.clone(),
        weather: config.weather_mode.clone(),
        app,
        hour,
        memory_summary,
        recent_lines,
    }
}

fn spawn_behavior_generation(
    engine: Arc<LlamaEngine>,
    tx: mpsc::Sender<String>,
    event: BehaviorEvent,
    context: BehaviorContext,
    clean_retry: bool,
) {
    let prompt = build_behavior_prompt(event, &context, clean_retry);
    tokio::spawn(async move {
        if let Ok(mut stream) = engine.generate_mutter_stream(&prompt).await {
            while let Some(chunk) = stream.next().await {
                if tx.send(chunk).await.is_err() {
                    return;
                }
            }
            let _ = tx.send("[DONE]".to_string()).await;
        } else {
            let _ = tx.send("[OFFLINE]".to_string()).await;
        }
    });
}

fn fallback_for_behavior(
    event: BehaviorEvent,
    pet_style: &str,
    weather_mode: &str,
    app: &str,
    hour: u32,
) -> String {
    match event {
        BehaviorEvent::Startup => startup_fallback_line(pet_style).to_string(),
        BehaviorEvent::Touch => {
            let lines = touch_response_lines(pet_style);
            lines[get_pseudo_random_index(lines.len())].to_string()
        }
        BehaviorEvent::Ambient => get_offline_mutter(app, hour, pet_style),
        BehaviorEvent::Dream => style_speech_profile(pet_style).sleep_line.to_string(),
        BehaviorEvent::BodyChanged => style_switch_line(pet_style).to_string(),
        BehaviorEvent::WeatherChanged => weather_feedback_line(pet_style, weather_mode).to_string(),
        BehaviorEvent::Renamed => "这个新称呼，我记下了。".to_string(),
        BehaviorEvent::PersonaShift => "我试着用新的语气陪你。".to_string(),
        BehaviorEvent::MemoryReset => "我会从现在开始重新认识你。".to_string(),
    }
}

fn queue_behavior(pending: &mut Option<PendingBehavior>, request: PendingBehavior) {
    let should_replace = pending
        .as_ref()
        .map(|current| request.event.priority() >= current.event.priority())
        .unwrap_or(true);
    if should_replace {
        *pending = Some(request);
    }
}

fn queue_contextual_behavior(
    pending: &mut Option<PendingBehavior>,
    event: BehaviorEvent,
    config: &AppConfig,
    memory: &Arc<std::sync::Mutex<PetMemory>>,
    observer_state: &Arc<std::sync::Mutex<(String, u32)>>,
) {
    let (app, hour) = observer_state
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_else(|_| ("未知应用".to_string(), 12));
    queue_behavior(
        pending,
        PendingBehavior {
            event,
            context: behavior_context(config, memory, app, hour),
        },
    );
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // 1. Load config
    let mut config = AppConfig::load_or_create();

    // 1.5 Load memory diary
    let memory = Arc::new(std::sync::Mutex::new(PetMemory::load_or_create()));

    // 2. Setup Winit Event Loop (must initialize before tray-icon on macOS)
    let event_loop = EventLoop::new()?;

    // Build System Tray
    let (_tray_icon, tray_controls) = {
        let menu = Menu::new();
        let model_label = match config.model_path.as_ref() {
            Some(path) if path.is_file() => "对话模式：本地模型",
            Some(_) => "对话模式：模型文件不存在",
            None => "对话模式：未配置模型 · 可离线陪伴",
        };
        menu.append(&MenuItem::with_id("model_status", model_label, false, None)).unwrap();
        menu.append(&MenuItem::with_id("action_chat", "和桌宠说句话…", true, None)).unwrap();
        menu.append(&MenuItem::with_id("action_last_message", "查看完整消息…", true, None)).unwrap();
        menu.append(&MenuItem::with_id("action_help", "使用帮助…", true, None)).unwrap();
        menu.append(&PredefinedMenuItem::separator()).unwrap();

        let mutter_check = CheckMenuItem::with_id(
            "mutter_enabled",
            "主动说话（含梦话）",
            true,
            config.mutter_enabled,
            None,
        );
        menu.append(&mutter_check).unwrap();

        let frequency_submenu = Submenu::with_id("behavior_frequency", "互动频率", true);
        let mutter_submenu = Submenu::with_id("mutter_frequency", "自言自语间隔", true);
        let mutter_60 = CheckMenuItem::with_id(
            "mutter_interval_60",
            "约 1 分钟",
            true,
            config.mutter_interval_secs == 60,
            None,
        );
        let mutter_180 = CheckMenuItem::with_id(
            "mutter_interval_180",
            "约 3 分钟",
            true,
            config.mutter_interval_secs == 180,
            None,
        );
        let mutter_300 = CheckMenuItem::with_id(
            "mutter_interval_300",
            "约 5 分钟",
            true,
            config.mutter_interval_secs == 300,
            None,
        );
        mutter_submenu
            .append_items(&[&mutter_60, &mutter_180, &mutter_300])
            .unwrap();

        let touch_submenu = Submenu::with_id("touch_frequency", "触摸回应冷却", true);
        let touch_5 = CheckMenuItem::with_id(
            "touch_cooldown_5",
            "5 秒",
            true,
            config.touch_response_cooldown_secs == 5,
            None,
        );
        let touch_15 = CheckMenuItem::with_id(
            "touch_cooldown_15",
            "15 秒",
            true,
            config.touch_response_cooldown_secs == 15,
            None,
        );
        let touch_30 = CheckMenuItem::with_id(
            "touch_cooldown_30",
            "30 秒",
            true,
            config.touch_response_cooldown_secs == 30,
            None,
        );
        touch_submenu
            .append_items(&[&touch_5, &touch_15, &touch_30])
            .unwrap();

        let dream_submenu = Submenu::with_id("dream_frequency", "梦话间隔", true);
        let dream_45 = CheckMenuItem::with_id(
            "dream_interval_45",
            "约 45 秒",
            true,
            config.dream_interval_secs == 45,
            None,
        );
        let dream_75 = CheckMenuItem::with_id(
            "dream_interval_75",
            "约 75 秒",
            true,
            config.dream_interval_secs == 75,
            None,
        );
        let dream_120 = CheckMenuItem::with_id(
            "dream_interval_120",
            "约 2 分钟",
            true,
            config.dream_interval_secs == 120,
            None,
        );
        dream_submenu
            .append_items(&[&dream_45, &dream_75, &dream_120])
            .unwrap();
        frequency_submenu
            .append_items(&[&mutter_submenu, &touch_submenu, &dream_submenu])
            .unwrap();
        menu.append(&frequency_submenu).unwrap();

        let persona_submenu = Submenu::with_id("persona", "切换桌宠人设", true);
        let p_default = CheckMenuItem::with_id(
            "persona_default",
            "软萌活泼 (默认)",
            true,
            config.persona == "default",
            None,
        );
        let p_tsundere = CheckMenuItem::with_id(
            "persona_tsundere",
            "傲娇嘴硬",
            true,
            config.persona == "tsundere",
            None,
        );
        let p_toxic = CheckMenuItem::with_id(
            "persona_toxic",
            "冷酷毒舌",
            true,
            config.persona == "toxic",
            None,
        );
        let p_moe = CheckMenuItem::with_id(
            "persona_moe",
            "极度粘人",
            true,
            config.persona == "moe",
            None,
        );
        persona_submenu
            .append_items(&[&p_default, &p_tsundere, &p_toxic, &p_moe])
            .unwrap();
        menu.append(&persona_submenu).unwrap();

        let style_submenu = Submenu::with_id("pet_style", "切换桌宠外观", true);
        let s_nimbus = CheckMenuItem::with_id(
            "style_nimbus",
            "粉蓝小云朵 (Nimbus)",
            true,
            config.pet_style == "nimbus",
            None,
        );
        let s_mochi = CheckMenuItem::with_id(
            "style_mochi",
            "抹茶猫咪麻薯 (Mochi)",
            true,
            config.pet_style == "mochi",
            None,
        );
        let s_starlit = CheckMenuItem::with_id(
            "style_starlit",
            "太空章鱼 (Starlit)",
            true,
            config.pet_style == "starlit",
            None,
        );
        let s_crab = CheckMenuItem::with_id(
            "style_crab",
            "红螯小螃蟹 (Crab)",
            true,
            config.pet_style == "crab",
            None,
        );
        let s_luma = CheckMenuItem::with_id(
            "style_luma",
            "月灯兔 (Luma)",
            true,
            config.pet_style == "luma",
            None,
        );
        style_submenu
            .append_items(&[&s_nimbus, &s_mochi, &s_starlit, &s_crab, &s_luma])
            .unwrap();
        menu.append(&style_submenu).unwrap();

        let weather_submenu = Submenu::with_id("weather_sub", "切换桌宠天气", true);
        let w_sunny = CheckMenuItem::with_id(
            "weather_sunny",
            "☀️ 晴天",
            true,
            config.weather_mode == "sunny",
            None,
        );
        let w_windy = CheckMenuItem::with_id(
            "weather_windy",
            "💨 狂风",
            true,
            config.weather_mode == "windy",
            None,
        );
        let w_rainy = CheckMenuItem::with_id(
            "weather_rainy",
            "🌧️ 细雨",
            true,
            config.weather_mode == "rainy",
            None,
        );
        let w_snowy = CheckMenuItem::with_id(
            "weather_snowy",
            "❄️ 飘雪",
            true,
            config.weather_mode == "snowy",
            None,
        );
        weather_submenu
            .append_items(&[&w_sunny, &w_windy, &w_rainy, &w_snowy])
            .unwrap();
        menu.append(&weather_submenu).unwrap();

        let font_submenu = Submenu::with_id("font_size", "调整字号大小", true);
        let f_20 = CheckMenuItem::with_id(
            "font_20",
            "20 像素",
            true,
            (config.font_size - 20.0).abs() < f32::EPSILON,
            None,
        );
        let f_24 = CheckMenuItem::with_id(
            "font_24",
            "24 像素 (默认)",
            true,
            (config.font_size - 24.0).abs() < f32::EPSILON,
            None,
        );
        let f_28 = CheckMenuItem::with_id(
            "font_28",
            "28 像素",
            true,
            (config.font_size - 28.0).abs() < f32::EPSILON,
            None,
        );
        let f_32 = CheckMenuItem::with_id(
            "font_32",
            "32 像素",
            true,
            (config.font_size - 32.0).abs() < f32::EPSILON,
            None,
        );
        font_submenu
            .append_items(&[&f_20, &f_24, &f_28, &f_32])
            .unwrap();
        menu.append(&font_submenu).unwrap();

        let size_submenu = Submenu::with_id("pet_size_sub", "调整桌宠大小", true);
        let s_150 = CheckMenuItem::with_id(
            "size_150",
            "小 (150 像素)",
            true,
            config.pet_size == 150,
            None,
        );
        let s_200 = CheckMenuItem::with_id(
            "size_200",
            "中 (200 像素)",
            true,
            config.pet_size == 200,
            None,
        );
        let s_250 = CheckMenuItem::with_id(
            "size_250",
            "大 (250 像素)",
            true,
            config.pet_size == 250,
            None,
        );
        let s_300 = CheckMenuItem::with_id(
            "size_300",
            "特大 (300 像素)",
            true,
            config.pet_size == 300,
            None,
        );
        let s_350 = CheckMenuItem::with_id(
            "size_350",
            "巨型 (350 像素)",
            true,
            config.pet_size == 350,
            None,
        );
        size_submenu
            .append_items(&[&s_150, &s_200, &s_250, &s_300, &s_350])
            .unwrap();
        menu.append(&size_submenu).unwrap();

        let config_submenu = Submenu::with_id("config_sub", "系统参数设置", true);
        let c_clear = MenuItem::with_id("action_clear", "清空对话历史", true, None);
        let c_sleep_30 = CheckMenuItem::with_id(
            "sleep_30",
            "显存休眠 30 秒",
            true,
            config.idle_offload_secs == 30,
            None,
        );
        let c_sleep_60 = CheckMenuItem::with_id(
            "sleep_60",
            "显存休眠 60 秒",
            true,
            config.idle_offload_secs == 60,
            None,
        );
        let c_sleep_120 = CheckMenuItem::with_id(
            "sleep_120",
            "显存休眠 120 秒",
            true,
            config.idle_offload_secs == 120,
            None,
        );
        config_submenu
            .append_items(&[&c_clear, &c_sleep_30, &c_sleep_60, &c_sleep_120])
            .unwrap();
        menu.append(&config_submenu).unwrap();

        let rename_item = MenuItem::with_id("action_rename", "🖊️ 修改桌宠名字", true, None);
        menu.append(&rename_item).unwrap();

        let memory_submenu = Submenu::with_id("memory_sub", "💾 主人记忆管理", true);
        let m_show = MenuItem::with_id("memory_show_info", "查看记忆详情", true, None);
        let m_open = MenuItem::with_id("memory_open_dir", "打开记忆目录", true, None);
        let m_clear = MenuItem::with_id("memory_clear", "清空所有记忆", true, None);
        memory_submenu
            .append_items(&[&m_show, &m_open, &m_clear])
            .unwrap();
        menu.append(&memory_submenu).unwrap();

        menu.append(&PredefinedMenuItem::separator()).unwrap();
        let exit_item = MenuItem::with_id("action_exit", "退出桌宠", true, None);
        menu.append(&exit_item).unwrap();

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("灵霓 Nimbus 桌宠控制面板")
            .with_icon(create_tray_icon())
            .with_icon_as_template(true)
            .build()
            .unwrap();
        let controls = TrayControls {
            mutter_enabled: mutter_check,
            mutter_intervals: [mutter_60, mutter_180, mutter_300],
            touch_cooldowns: [touch_5, touch_15, touch_30],
            dream_intervals: [dream_45, dream_75, dream_120],
            personas: [p_default, p_tsundere, p_toxic, p_moe],
            styles: [s_nimbus, s_mochi, s_starlit, s_crab, s_luma],
            weather_modes: [w_sunny, w_windy, w_rainy, w_snowy],
            font_sizes: [f_20, f_24, f_28, f_32],
            pet_sizes: [s_150, s_200, s_250, s_300, s_350],
            offload_timeouts: [c_sleep_30, c_sleep_60, c_sleep_120],
        };
        controls.sync(&config);
        (tray_icon, controls)
    };

    // Pet Window Logical Dimension (tight wrapping)
    let (pet_win_width, pet_win_height) =
        pet_window_logical_size(config.pet_size, &config.pet_style);

    // Bubble Window Initial Logical Dimension (will dynamically adapt)
    let bubble_win_width = BUBBLE_INITIAL_WIDTH;
    let bubble_win_height = BUBBLE_INITIAL_HEIGHT;

    // 3. Setup Window (Pet Window)
    let window = WindowBuilder::new()
        .with_title("Nimbus Desk Pet Widget")
        .with_decorations(false) // Borderless
        .with_transparent(true) // Transparent background
        .with_resizable(false) // Lock size
        .with_window_level(if config.always_on_top {
            winit::window::WindowLevel::AlwaysOnTop
        } else {
            winit::window::WindowLevel::Normal
        })
        .with_inner_size(winit::dpi::LogicalSize::new(
            pet_win_width as f64,
            pet_win_height as f64,
        ))
        .build(&event_loop)?;

    // Setup Bubble Window (Click-through, hidden initially)
    let bubble_window = WindowBuilder::new()
        .with_title("Nimbus Desk Pet Bubble")
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_window_level(if config.always_on_top {
            winit::window::WindowLevel::AlwaysOnTop
        } else {
            winit::window::WindowLevel::Normal
        })
        .with_inner_size(winit::dpi::LogicalSize::new(
            bubble_win_width as f64,
            bubble_win_height as f64,
        ))
        .with_visible(false)
        .build(&event_loop)?;

    if let Err(err) = bubble_window.set_cursor_hittest(false) {
        eprintln!("Failed to enable bubble click-through: {err:?}");
    }

    // Position windows in bottom right corner by default
    if let Some(monitor) = window.current_monitor() {
        let screen_size = monitor.size();
        let scale_factor = window.scale_factor();
        let origin = monitor.position();
        let (x, y) = placement::default_pet_position(
            placement::ScreenRect { x: origin.x, y: origin.y, width: screen_size.width, height: screen_size.height },
            ((pet_win_width as f64 * scale_factor) as u32, (pet_win_height as f64 * scale_factor) as u32),
            (50, 100),
        );
        window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
    }

    let window = Arc::new(window);
    let window_clone = window.clone();
    let bubble_window = Arc::new(bubble_window);
    let bubble_window_clone = bubble_window.clone();

    // 3. Initialize Wgpu States
    let mut wgpu_state = WgpuState::new(window.clone(), pet_win_width, pet_win_height).await?;
    let mut bubble_wgpu_state =
        WgpuState::new(bubble_window.clone(), bubble_win_width, bubble_win_height).await?;

    // 4. Initialize custom 2D Renderers
    let pet_initial_size = window.inner_size();
    let bubble_initial_size = bubble_window.inner_size();

    let mut pet_renderer = PetRenderer::new(
        pet_initial_size.width.max(1),
        pet_initial_size.height.max(1),
    );
    pet_renderer.draw_bubble_enabled = false;
    pet_renderer.draw_pet_enabled = true;
    pet_renderer.active_style = config.pet_style.clone();
    pet_renderer.active_weather = config.weather_mode.clone();
    sync_renderer_to_window(&mut pet_renderer, &window, config.font_size);

    let mut bubble_renderer = PetRenderer::new(
        bubble_initial_size.width.max(1),
        bubble_initial_size.height.max(1),
    );
    bubble_renderer.draw_bubble_enabled = true;
    bubble_renderer.draw_pet_enabled = false;
    bubble_renderer.active_style = config.pet_style.clone();
    bubble_renderer.active_weather = config.weather_mode.clone();
    sync_renderer_to_window(&mut bubble_renderer, &bubble_window, config.font_size);

    // 5. Initialize in-process LlamaEngine with Smart Idle Offload config
    let llama_engine = if let Some(model_path) = &config.model_path {
        match LlamaEngine::new(model_path.clone(), config.idle_offload_secs) {
            Ok(engine) => {
                println!("LlamaEngine initialized successfully (In-Process mode)!");
                let info = engine.resource_info();
                println!(
                    "[Nimbus] Model resource baseline: path={}, file={} bytes, online={}, active_context={} bytes",
                    info.model_path.display(),
                    info.model_file_bytes,
                    info.model_online,
                    info.active_metal_context_bytes
                );
                Some(Arc::new(engine))
            }
            Err(e) => {
                eprintln!("Failed to initialize in-process LlamaEngine: {:?}", e);
                None
            }
        }
    } else {
        println!("No local model configured in config.toml. Running in GUI demonstration mode.");
        None
    };

    // Shared State for User Activity Observer (AppName, LocalHour)
    let observer_state = Arc::new(std::sync::Mutex::new(("未知应用".to_string(), 12u32)));
    let observer_state_clone = observer_state.clone();

    // Spawn Background Activity Observer Task (Every 45 seconds)
    let memory_clone = memory.clone();
    tokio::spawn(async move {
        loop {
            let (app, hour) = observe_user_activity().await;
            if let Ok(mut guard) = observer_state_clone.lock() {
                *guard = (app.clone(), hour);
            }
            if let Ok(mut mem) = memory_clone.lock() {
                mem.record_app(&app, hour);
            }
            tokio::time::sleep(std::time::Duration::from_secs(45)).await;
        }
    });

    // Interaction & Timer states
    let mut cursor_position = winit::dpi::PhysicalPosition::new(0.0, 0.0);
    let mut last_interactive_time = std::time::Instant::now();
    let mut last_hover_trigger_time: Option<std::time::Instant> = None;
    let mut was_pointer_on_pet = false;
    let mut is_pointer_on_pet = false;
    let mut is_dragging_pet = false;
    let mut pet_hittest_enabled = true;
    let mut surprised_timer: Option<std::time::Instant> = None;
    let mut last_pointer_sample: Option<(std::time::Instant, f64, f64)> = None;
    let state_now = std::time::Instant::now();
    let mut last_global_pointer_position: Option<(i32, i32)> = None;
    let mut last_global_pointer_motion = state_now;
    let mut last_state_change = state_now;
    let mut state_machine = StateMachine::new(PetState::Idle, state_now);
    let mut was_sleeping = false;

    // Channels for text streaming and terminal input
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(100);
    let ui_input_tx = stdin_tx.clone();
    let dialogs = desktop::DesktopDialogs::new();
    let mut last_message = startup_fallback_line(&config.pet_style).to_string();
    let mut active_response = String::new();
    let mut active_generation: Option<ActiveGeneration> = None;
    let mut pending_behavior: Option<PendingBehavior> = None;
    let mut bubble_reveal: Option<TextReveal> = None;
    let entropy = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut next_mutter_at = state_now + jittered_interval(config.mutter_interval_secs, entropy);
    let mut next_dream_at =
        state_now + jittered_interval(config.dream_interval_secs, entropy.rotate_left(17));

    // 6. Spawn Stdin non-blocking reader task
    let idle_sec_hint = config.idle_offload_secs;
    let model_configured = llama_engine.is_some();
    let pet_name_clone = config.pet_name.clone();
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut reader = BufReader::new(tokio::io::stdin());
        let mut line = String::new();

        println!("==================================================");
        println!("      桌宠 {} 极速推理与资源优化版       ", pet_name_clone);
        println!("==================================================");
        println!("* {}", if model_configured { "本地模型已配置。" } else { "当前为离线陪伴模式；模型配置方法见托盘“使用帮助”。" });
        println!("* 闲置 {} 秒将自动释放 GPU 显存 / 内存。", idle_sec_hint);
        println!("* 在此终端输入文字按 [回车] 即可与屏幕桌宠聊天。");
        println!("* 输入 /clear 清空上下文，输入 /exit 退出程序。");
        println!("==================================================");

        loop {
            line.clear();
            print!("\n>> ");
            use std::io::Write;
            let _ = std::io::stdout().flush();

            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        let _ = stdin_tx.send(trimmed.to_string()).await;
                    }
                }
                Err(e) => {
                    eprintln!("Error reading stdin: {:?}", e);
                    break;
                }
            }
        }
    });

    // Redraw rate limiting states
    let mut last_draw_time = std::time::Instant::now();
    let mut bubble_timer: Option<std::time::Instant> = None;
    let mut bubble_show_duration = std::time::Duration::from_secs(config.bubble_timeout_secs);

    // Thinking Block Stream Filter states
    let mut filter = think_filter::ThinkBlockFilter::new(config.expose_think, false);
    let mut in_thinking_block = false;

    if let Some(ref engine) = llama_engine {
        filter = think_filter::ThinkBlockFilter::new(config.expose_think, false);
        active_response.clear();
        let context = behavior_context(&config, &memory, "桌面".to_string(), 12);
        spawn_behavior_generation(
            engine.clone(),
            tx.clone(),
            BehaviorEvent::Startup,
            context.clone(),
            false,
        );
        active_generation = Some(ActiveGeneration {
            kind: GenerationKind::Behavior(BehaviorEvent::Startup),
            context: Some(context),
            retry_count: 0,
            started_at: std::time::Instant::now(),
        });
    } else {
        bubble_renderer.set_text(startup_fallback_line(&config.pet_style));
        bubble_timer = Some(std::time::Instant::now());
    }

    // Trigger initial redraw
    window_clone.request_redraw();
    bubble_window_clone.request_redraw();

    let window_for_run = window_clone.clone();

    // 7. Run Event Loop
    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Wait);

        while let Some(result) = dialogs.try_recv() {
            match result {
                desktop::DialogResult::Input(desktop::InputKind::Chat, text) => {
                    if ui_input_tx.try_send(text).is_err() {
                        dialogs.show("消息队列已满", "请等当前回复完成后再试。");
                    }
                }
                desktop::DialogResult::Input(desktop::InputKind::Rename, name) => {
                    if name.chars().count() > 40 {
                        dialogs.show("名字太长了", "请使用不超过 40 个字的名字。");
                        continue;
                    }
                    config.pet_name = name;
                    config.save();
                    queue_contextual_behavior(&mut pending_behavior, BehaviorEvent::Renamed, &config, &memory, &observer_state);
                    window_for_run.request_redraw();
                }
                desktop::DialogResult::Failed => {
                    eprintln!("Unable to open native dialog; terminal input remains available.");
                    bubble_renderer.set_text("原生对话框未能打开，请稍后再试，或在前台终端输入消息。");
                    bubble_timer = Some(std::time::Instant::now());
                }
            }
        }
        if !active_response.trim().is_empty() && active_generation.as_ref().is_none_or(|session| session.kind == GenerationKind::Chat) {
            last_message.clone_from(&active_response);
        }

        // Process incoming Stdin commands
        while active_generation.is_none() {
            let Ok(input_prompt) = stdin_rx.try_recv() else {
                break;
            };
            if input_prompt == "/exit" {
                println!("Exiting Nimbus Desk Pet. Goodbye!");
                elwt.exit();
                return;
            }
            if input_prompt == "/clear" {
                if let Some(ref engine) = llama_engine {
                    let _ = engine.clear_history();
                }
                active_response.clear();
                bubble_renderer.set_text("✨ 对话上下文已重置！");
                bubble_timer = Some(std::time::Instant::now());
                bubble_show_duration = std::time::Duration::from_secs(6);
                last_interactive_time = std::time::Instant::now();
                window_for_run.request_redraw();
                println!("\n>> 历史已重置。");
                continue;
            }

            if let Some(ref engine) = llama_engine {
                filter = think_filter::ThinkBlockFilter::new(config.expose_think, false);
                in_thinking_block = false;
                active_response.clear();
                bubble_renderer.set_text("");
                bubble_timer = None;
                bubble_reveal = None;
                last_interactive_time = std::time::Instant::now();
                let entropy = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                next_mutter_at = last_interactive_time
                    + jittered_interval(config.mutter_interval_secs, entropy);
                window_for_run.request_redraw();

                if let Ok(mut mem) = memory.lock() {
                    mem.increment_interactions();
                }

                let memory_desc = memory
                    .lock()
                    .map(|mem| mem.describe_for_prompt())
                    .unwrap_or_else(|_| "你还没有关于屏幕前的人的日常偏好记忆。".to_string());

                let persona_prompt = get_persona_prompt(&config.persona, &config.pet_style, &config.pet_name, &config.weather_mode);
                let prompt = format!(
                    "人设设定：{}。{}。屏幕前的人对你说：\"{}\"。",
                    persona_prompt, memory_desc, input_prompt
                );

                let engine_clone = engine.clone();
                let tx_clone = tx.clone();

                tokio::spawn(async move {
                    if let Ok(mut stream) = engine_clone.generate_stream(&prompt).await {
                        while let Some(chunk) = stream.next().await {
                            let _ = tx_clone.send(chunk).await;
                        }
                        let _ = tx_clone.send("[DONE]".to_string()).await;
                    } else {
                        let _ = tx_clone.send("[OFFLINE]".to_string()).await;
                    }
                });
                active_generation = Some(ActiveGeneration {
                    kind: GenerationKind::Chat,
                    context: None,
                    retry_count: 0,
                    started_at: std::time::Instant::now(),
                });
            } else {
                active_response = desktop::NO_MODEL_MESSAGE.to_string();
                bubble_renderer.set_text(&active_response);
                bubble_timer = Some(std::time::Instant::now());
                bubble_show_duration = std::time::Duration::from_secs(12);
                bubble_reveal = None;
                window_for_run.request_redraw();
            }
        }

        // Process streamed tokens from LLM Engine (includes Chat & Mutter)
        while let Ok(new_text) = rx.try_recv() {
            if new_text == "[DONE]" {
                let outputs = filter.flush();
                for out in outputs {
                    if out.event == "think" {
                        if !in_thinking_block {
                            in_thinking_block = true;
                            active_response.push_str("[思考] ");
                        }
                        active_response.push_str(&out.content);
                    } else {
                        if in_thinking_block {
                            in_thinking_block = false;
                            active_response.push_str("\n\n");
                        }
                        active_response.push_str(&out.content);
                    }
                }
                println!();
                let Some(session) = active_generation.take() else {
                    active_response.clear();
                    continue;
                };
                let elapsed_ms = session.started_at.elapsed().as_millis();

                match session.kind {
                    GenerationKind::Behavior(BehaviorEvent::Ambient | BehaviorEvent::Dream) if !config.mutter_enabled => {
                        active_response.clear();
                        bubble_renderer.set_text("");
                        bubble_timer = None;
                        bubble_reveal = None;
                    }
                    GenerationKind::Chat => {
                        active_response = active_response.trim().to_string();
                        if active_response.is_empty() {
                            active_response = desktop::EMPTY_RESPONSE_MESSAGE.to_string();
                        }
                        bubble_renderer.set_text(&active_response);
                        bubble_timer = Some(std::time::Instant::now());
                        bubble_show_duration =
                            std::time::Duration::from_secs(config.bubble_timeout_secs);
                        if !active_response.is_empty() {
                            if let Ok(mut mem) = memory.lock() {
                                mem.record_spoken_line(&active_response);
                            }
                        }
                        println!(
                            "[Generation] kind=chat status=accepted duration_ms={} chars={}",
                            elapsed_ms,
                            active_response.chars().count()
                        );
                    }
                    GenerationKind::Behavior(event) => {
                        let context = session.context.unwrap_or_else(|| {
                            behavior_context(
                                &config,
                                &memory,
                                "未知应用".to_string(),
                                12,
                            )
                        });
                        let cleaned = clean_behavior_line(
                            &active_response,
                            event,
                            &context.pet_style,
                            context.hour,
                            &context.recent_lines,
                        );

                        if cleaned.is_none() {
                            log::debug!(
                                "[Generation] kind={event:?} rejected={:?}",
                                active_response
                            );
                        }

                        if cleaned.is_none() && session.retry_count < event.max_retries() {
                            if let Some(ref engine) = llama_engine {
                                println!(
                                    "[Generation] kind={event:?} status=retry duration_ms={elapsed_ms}"
                                );
                                active_response.clear();
                                filter = think_filter::ThinkBlockFilter::new(
                                    config.expose_think,
                                    false,
                                );
                                in_thinking_block = false;
                                spawn_behavior_generation(
                                    engine.clone(),
                                    tx.clone(),
                                    event,
                                    context.clone(),
                                    true,
                                );
                                active_generation = Some(ActiveGeneration {
                                    kind: GenerationKind::Behavior(event),
                                    context: Some(context),
                                    retry_count: session.retry_count + 1,
                                    started_at: std::time::Instant::now(),
                                });
                                continue;
                            }
                        }

                        let used_fallback = cleaned.is_none();
                        active_response = cleaned.unwrap_or_else(|| {
                            fallback_for_behavior(
                                event,
                                &context.pet_style,
                                &context.weather,
                                &context.app,
                                context.hour,
                            )
                        });
                        bubble_renderer.set_text("");
                        bubble_timer = None;
                        bubble_show_duration = event.display_duration();
                        bubble_reveal = Some(TextReveal::new(
                            active_response.clone(),
                            std::time::Instant::now(),
                            bubble_show_duration,
                        ));
                        if let Ok(mut mem) = memory.lock() {
                            mem.record_spoken_line(&active_response);
                        }
                        println!(
                            "[Generation] kind={event:?} status={} duration_ms={} chars={}",
                            if used_fallback { "fallback" } else { "accepted" },
                            elapsed_ms,
                            active_response.chars().count()
                        );
                    }
                }
                window_for_run.request_redraw();
            } else if new_text == "[OFFLINE]" {
                let session = active_generation.take();
                if let Some(ref session) = session {
                    println!(
                        "[Generation] kind={:?} status=offline duration_ms={}",
                        session.kind,
                        session.started_at.elapsed().as_millis()
                    );
                }
                match session.map(|session| session.kind) {
                    Some(GenerationKind::Behavior(BehaviorEvent::Ambient | BehaviorEvent::Dream)) if !config.mutter_enabled => {
                        active_response.clear();
                        bubble_renderer.set_text("");
                        bubble_timer = None;
                        bubble_reveal = None;
                    }
                    Some(GenerationKind::Behavior(event)) => {
                        let (app, hour) = observer_state
                            .lock()
                            .map(|guard| guard.clone())
                            .unwrap_or_else(|_| ("未知应用".to_string(), 12));
                        active_response = fallback_for_behavior(
                            event,
                            &config.pet_style,
                            &config.weather_mode,
                            &app,
                            hour,
                        );
                        bubble_renderer.set_text("");
                        bubble_timer = None;
                        bubble_show_duration = event.display_duration();
                        bubble_reveal = Some(TextReveal::new(
                            active_response.clone(),
                            std::time::Instant::now(),
                            bubble_show_duration,
                        ));
                    }
                    _ => {
                        active_response = desktop::EMPTY_RESPONSE_MESSAGE.to_string();
                        bubble_renderer.set_text(&active_response);
                        bubble_timer = Some(std::time::Instant::now());
                        bubble_show_duration = std::time::Duration::from_secs(12);
                        bubble_reveal = None;
                    }
                }
                window_for_run.request_redraw();
            } else {
                let is_chat = active_generation
                    .as_ref()
                    .is_some_and(|session| session.kind == GenerationKind::Chat);
                if is_chat {
                    print!("{}", new_text);
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                }

                let outputs = filter.feed(&new_text);
                for out in outputs {
                    if out.event == "think" {
                        if !in_thinking_block {
                            in_thinking_block = true;
                            active_response.push_str("[思考] ");
                        }
                        active_response.push_str(&out.content);
                    } else {
                        if in_thinking_block {
                            in_thinking_block = false;
                            active_response.push_str("\n\n");
                        }
                        active_response.push_str(&out.content);
                    }
                }
                if is_chat {
                    bubble_renderer.set_text(&active_response);
                    window_for_run.request_redraw();
                }
            }
        }

        let reveal_now = std::time::Instant::now();
        let mut completed_reveal_duration = None;
        if let Some(reveal) = bubble_reveal.as_mut() {
            if let Some(visible_text) = reveal.advance(reveal_now) {
                bubble_renderer.set_text(&visible_text);
                window_for_run.request_redraw();
                bubble_window_clone.request_redraw();
            }
            if reveal.is_complete() {
                completed_reveal_duration = Some(reveal.display_duration);
            }
        }
        if let Some(duration) = completed_reveal_duration {
            bubble_reveal = None;
            bubble_timer = Some(reveal_now);
            bubble_show_duration = duration;
        }

        // Handle auto-fade timeout for chat bubble
        if let Some(timer) = bubble_timer {
            if std::time::Instant::now().duration_since(timer) > bubble_show_duration {
                active_response.clear();
                bubble_renderer.set_text("");
                bubble_timer = None;
                window_for_run.request_redraw();
            }
        }

        // Position & Size alignment step
        let pet_scale = window_for_run.scale_factor() as f32;
        let bubble_scale = bubble_window_clone.scale_factor() as f32;

        if (pet_renderer.scale_factor - pet_scale).abs() > f32::EPSILON {
            pet_renderer.scale_factor = pet_scale;
            pet_renderer.set_font_size(config.font_size);
        }
        if (bubble_renderer.scale_factor - bubble_scale).abs() > f32::EPSILON {
            bubble_renderer.scale_factor = bubble_scale;
            bubble_renderer.set_font_size(config.font_size);
        }

        let chat_is_streaming = active_generation
            .as_ref()
            .is_some_and(|session| session.kind == GenerationKind::Chat);
        if (bubble_timer.is_some() || bubble_reveal.is_some() || chat_is_streaming)
            && bubble_renderer.has_text()
        {
            if let (Some(monitor), Ok(pos)) = (window_for_run.current_monitor(), window_for_run.outer_position()) {
                let origin = monitor.position();
                let size = monitor.size();
                let pet_size = window_for_run.inner_size();
                let screen = placement::ScreenRect { x: origin.x, y: origin.y, width: size.width, height: size.height };
                let pet = placement::ScreenRect { x: pos.x, y: pos.y, width: pet_size.width, height: pet_size.height };
                let (width, height) = placement::bubble_limits(screen, pet, (8.0 * bubble_scale).round() as u32);
                bubble_renderer.set_bubble_limits(width as f32, height as f32, 8);
            }
            let (b_w_phys, b_h_phys) = bubble_renderer.measure_bubble_size();
            if b_w_phys > 0.0 && b_h_phys > 0.0 {
                let b_w_log = b_w_phys as f64 / bubble_scale as f64;
                let b_h_log = b_h_phys as f64 / bubble_scale as f64;

                let current_size = bubble_window_clone.inner_size();
                let mut effective_bubble_size = current_size;
                if (current_size.width as f64 - b_w_phys as f64).abs() > 1.0
                    || (current_size.height as f64 - b_h_phys as f64).abs() > 1.0 {
                    let requested = bubble_window_clone
                        .request_inner_size(winit::dpi::LogicalSize::new(b_w_log, b_h_log));
                    effective_bubble_size = requested.unwrap_or_else(|| {
                        winit::dpi::PhysicalSize::new(
                            b_w_phys.round().max(1.0) as u32,
                            b_h_phys.round().max(1.0) as u32,
                        )
                    });
                    bubble_wgpu_state.resize(
                        effective_bubble_size.width.max(1),
                        effective_bubble_size.height.max(1),
                    );
                    bubble_renderer.resize(
                        effective_bubble_size.width.max(1),
                        effective_bubble_size.height.max(1),
                    );
                }

                if let Ok(pet_pos) = window_for_run.outer_position() {
                    let pet_size = window_for_run.inner_size();

                    if let Some(monitor) = window_for_run.current_monitor() {
                        let origin = monitor.position();
                        let size = monitor.size();
                        let placed = placement::place_bubble(
                            placement::ScreenRect { x: origin.x, y: origin.y, width: size.width, height: size.height },
                            placement::ScreenRect { x: pet_pos.x, y: pet_pos.y, width: pet_size.width, height: pet_size.height },
                            (effective_bubble_size.width, effective_bubble_size.height),
                            (8.0 * bubble_scale).round() as u32,
                        );
                        bubble_renderer.bubble_on_bottom = placed.on_bottom;
                        bubble_renderer.bubble_tail_x = Some(placed.tail_x);
                        bubble_window_clone.set_outer_position(winit::dpi::PhysicalPosition::new(placed.x, placed.y));
                    }
                    if !bubble_window_clone.is_visible().unwrap_or(false) {
                        bubble_window_clone.set_visible(true);
                    }
                    bubble_window_clone.request_redraw();
                }
            } else {
                bubble_window_clone.set_visible(false);
            }
        } else {
            if bubble_window_clone.is_visible().unwrap_or(false) {
                bubble_window_clone.set_visible(false);
            }
        }

        match event {
            Event::WindowEvent { event, window_id } => {
                if window_id == window_for_run.id() {
                    match event {
                        WindowEvent::CloseRequested => {
                            elwt.exit();
                        }
                        WindowEvent::Resized(size) => {
                            wgpu_state.resize(size.width, size.height);
                            pet_renderer.scale_factor = window_for_run.scale_factor() as f32;
                            pet_renderer.resize(size.width.max(1), size.height.max(1));
                            pet_renderer.set_font_size(config.font_size);
                            window_for_run.request_redraw();
                        }
                        WindowEvent::ScaleFactorChanged { .. } => {
                            let size = sync_renderer_to_window(
                                &mut pet_renderer,
                                &window_for_run,
                                config.font_size,
                            );
                            wgpu_state.resize(size.width.max(1), size.height.max(1));
                            window_for_run.request_redraw();
                        }
                        WindowEvent::RedrawRequested => {
                            use mouse_position::mouse_position::Mouse;
                            if let Mouse::Position { x, y } = Mouse::get_mouse_position() {
                                if let Ok(win_pos) = window_for_run.outer_position() {
                                    let wx = x as f64 - win_pos.x as f64;
                                    let wy = y as f64 - win_pos.y as f64;
                                    set_pet_pointer_from_physical(
                                        &mut pet_renderer,
                                        window_for_run.inner_size(),
                                        wx,
                                        wy,
                                    );
                                }
                            }

                            let now_inst = std::time::Instant::now();
                            let (app, hour) = observer_state
                                .lock()
                                .map(|guard| guard.clone())
                                .unwrap_or_else(|_| ("未知应用".to_string(), 12));
                            let dreaming = active_generation.as_ref().is_some_and(|session| {
                                session.kind
                                    == GenerationKind::Behavior(BehaviorEvent::Dream)
                            });
                            let target_state = decide_state(&StateContext {
                                surprised: surprised_timer.is_some_and(|started| {
                                    now_inst.duration_since(started)
                                        < std::time::Duration::from_millis(900)
                                }),
                                pointer_on_pet: is_pointer_on_pet,
                                dreaming,
                                muttering: active_generation.is_some(),
                                thinking: in_thinking_block
                                    || (active_generation.is_some()
                                        && active_response.trim().is_empty()),
                                idle_duration: now_inst.duration_since(last_interactive_time),
                                active_app: &app,
                                hour,
                            });
                            let previous_state = state_machine.current();
                            let current_state = state_machine.update(target_state, now_inst);
                            if current_state != previous_state {
                                last_state_change = now_inst;
                            }
                            pet_renderer.pet_state = current_state.as_str().to_string();

                            pet_renderer.update_and_draw();

                            let skia_data = pet_renderer.data();
                            if let Err(e) = wgpu_state.render(skia_data) {
                                eprintln!("wgpu render error: {:?}", e);
                            }
                            last_draw_time = std::time::Instant::now();
                        }
                        WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                            let is_on_pet = pet_renderer.body_hit_test(cursor_position.x, cursor_position.y).unwrap_or(false);

                            if state == ElementState::Pressed {
                                if is_on_pet {
                                    is_dragging_pet = true;
                                    last_interactive_time = std::time::Instant::now();
                                    if let Err(e) = window_for_run.drag_window() {
                                        eprintln!("Failed to drag window: {:?}", e);
                                    }
                                }
                            } else if is_dragging_pet {
                                is_dragging_pet = false;
                                surprised_timer = Some(std::time::Instant::now());
                                pet_renderer.add_emotion(-0.02, 0.15);
                            }
                        }
                        WindowEvent::CursorMoved { position, .. } => {
                            cursor_position = position;

                            set_pet_pointer_from_physical(
                                &mut pet_renderer,
                                window_for_run.inner_size(),
                                position.x,
                                position.y,
                            );

                            let is_on_pet = pet_renderer.body_hit_test(position.x, position.y).unwrap_or(false);
                            is_pointer_on_pet = is_on_pet;
                            pet_renderer.pointer_inside = is_on_pet;

                            let now = std::time::Instant::now();
                            if let Some((sample_time, last_x, last_y)) = last_pointer_sample {
                                let elapsed = now.duration_since(sample_time).as_secs_f64();
                                if elapsed > 0.0 {
                                    let dx = position.x - last_x;
                                    let dy = position.y - last_y;
                                    let velocity = (dx * dx + dy * dy).sqrt() / elapsed;
                                    if is_on_pet && velocity > 1_800.0 {
                                        surprised_timer = Some(now);
                                        pet_renderer.add_emotion(-0.06, 0.28);
                                    }
                                }
                            }
                            last_pointer_sample = Some((now, position.x, position.y));

                            window_for_run.request_redraw();
                        }
                        WindowEvent::CursorLeft { .. } => {
                            pet_renderer.pointer_inside = false;
                            is_pointer_on_pet = false;
                            last_pointer_sample = None;
                            window_for_run.request_redraw();
                        }
                        _ => {}
                    }
                } else if window_id == bubble_window_clone.id() {
                    match event {
                        WindowEvent::Resized(size) => {
                            bubble_wgpu_state.resize(size.width.max(1), size.height.max(1));
                            bubble_renderer.scale_factor = bubble_window_clone.scale_factor() as f32;
                            bubble_renderer.resize(size.width.max(1), size.height.max(1));
                            bubble_renderer.set_font_size(config.font_size);
                            bubble_window_clone.request_redraw();
                        }
                        WindowEvent::ScaleFactorChanged { .. } => {
                            let size = sync_renderer_to_window(
                                &mut bubble_renderer,
                                &bubble_window_clone,
                                config.font_size,
                            );
                            bubble_wgpu_state.resize(size.width.max(1), size.height.max(1));
                            bubble_window_clone.request_redraw();
                        }
                        WindowEvent::RedrawRequested
                            if (bubble_timer.is_some()
                                || bubble_reveal.is_some()
                                || active_generation.as_ref().is_some_and(|session| {
                                    session.kind == GenerationKind::Chat
                                }))
                                && bubble_renderer.has_text() =>
                        {
                            bubble_renderer.update_and_draw();
                            let skia_data = bubble_renderer.data();
                            if let Err(e) = bubble_wgpu_state.render(skia_data) {
                                eprintln!("bubble wgpu render error: {:?}", e);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::AboutToWait => {
                let now = std::time::Instant::now();

                // Handle System Tray/Menubar events
                while let Ok(event) = MenuEvent::receiver().try_recv() {
                    let id = event.id.0.as_str();
                    match id {
                        "action_chat" => {
                            dialogs.input(desktop::InputKind::Chat, "和桌宠说句话", "想聊些什么？回复会显示在桌宠旁边。", "");
                        }
                        "action_last_message" => {
                            dialogs.show("完整消息", if last_message.is_empty() { "还没有消息。可以先和桌宠说句话。" } else { &last_message });
                        }
                        "action_help" => {
                            let mode = if llama_engine.is_some() { "已配置本地模型，休息时会自动释放资源。" } else { "目前为离线陪伴模式，可触摸、拖动和切换外观。" };
                            dialogs.show("Nimbus 使用帮助", &format!("{}\n\n• 聊天：托盘 → 和桌宠说句话\n• 重读：托盘 → 查看完整消息\n• 移动：按住桌宠身体拖动\n• 安静陪伴：关闭“主动说话（含梦话）”\n\n本地对话需要兼容的 GGUF 模型。在 ~/.nimbus_config.toml 中设置 model_path 为模型文件的绝对路径，保存后重启。无需模型也能离线陪伴。\n\n应用名称、活跃时段和成长记忆仅保存在 ~/.nimbus_memory.json；可在“主人记忆管理”中查看或清空。天气是手动设置，不代表实时天气。", mode));
                        }
                        "mutter_enabled" => {
                            config.mutter_enabled = tray_controls.mutter_enabled.is_checked();
                            if !config.mutter_enabled && pending_behavior.as_ref().is_some_and(|request| matches!(request.event, BehaviorEvent::Ambient | BehaviorEvent::Dream)) {
                                pending_behavior = None;
                            }
                            config.save();
                        }
                        "mutter_interval_60"
                        | "mutter_interval_180"
                        | "mutter_interval_300" => {
                            config.mutter_interval_secs = match id {
                                "mutter_interval_60" => 60,
                                "mutter_interval_300" => 300,
                                _ => 180,
                            };
                            config.save();
                            tray_controls.sync(&config);
                            let entropy = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos();
                            next_mutter_at = now
                                + jittered_interval(config.mutter_interval_secs, entropy);
                        }
                        "touch_cooldown_5"
                        | "touch_cooldown_15"
                        | "touch_cooldown_30" => {
                            config.touch_response_cooldown_secs = match id {
                                "touch_cooldown_5" => 5,
                                "touch_cooldown_30" => 30,
                                _ => 15,
                            };
                            config.save();
                            tray_controls.sync(&config);
                        }
                        "dream_interval_45"
                        | "dream_interval_75"
                        | "dream_interval_120" => {
                            config.dream_interval_secs = match id {
                                "dream_interval_45" => 45,
                                "dream_interval_120" => 120,
                                _ => 75,
                            };
                            config.save();
                            tray_controls.sync(&config);
                            let entropy = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos();
                            next_dream_at = now
                                + jittered_interval(
                                    config.dream_interval_secs,
                                    entropy.rotate_left(17),
                                );
                        }
                        "action_rename" => {
                            dialogs.input(desktop::InputKind::Rename, "设置桌宠名字", "请输入新名字（最多 40 个字）：", &config.pet_name);
                        }
                        "memory_show_info" => {
                            let path = PetMemory::get_path();
                            let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                            let size_str = if size_bytes > 1024 {
                                format!("{:.2} KB", size_bytes as f64 / 1024.0)
                            } else {
                                format!("{} Bytes", size_bytes)
                            };

                            let info_msg = {
                                if let Ok(mem) = memory.lock() {
                                    let top_apps = mem.get_top_apps();
                                    let top_hours = mem.get_top_hours();
                                    let active_desc = if top_apps.is_empty() {
                                        "暂无前台应用习惯记录。".to_string()
                                    } else {
                                        format!("主人最常用的前台应用是：{}；通常活跃时间是：{}。",
                                            top_apps.join(", "),
                                            top_hours.iter().map(|h| format!("{}点", h)).collect::<Vec<_>>().join("/")
                                        )
                                    };
                                    format!("记忆文件：~/.nimbus_memory.json\\n文件大小：{}\\n互动次数：{}次\\n\\n【日常习惯记忆】\\n{}",
                                        size_str, mem.total_interactions, active_desc)
                                } else {
                                    "无法读取记忆。".to_string()
                                }
                            };
                            dialogs.show("主人记忆日记详情", &info_msg.replace("\\n", "\n"));
                        }
                        "memory_open_dir" => {
                            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                            let _ = std::process::Command::new("open").arg(&home).status();
                        }
                        "memory_clear" => {
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            let cleared = if let Ok(mut mem) = memory.lock() {
                                mem.clear();
                                println!("\n>> 主人习惯记忆已重置。");
                                true
                            } else {
                                false
                            };
                            if cleared {
                                queue_contextual_behavior(
                                    &mut pending_behavior,
                                    BehaviorEvent::MemoryReset,
                                    &config,
                                    &memory,
                                    &observer_state,
                                );
                                window_for_run.request_redraw();
                            }
                        }
                        "persona_default" => {
                            config.persona = "default".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            queue_contextual_behavior(
                                &mut pending_behavior,
                                BehaviorEvent::PersonaShift,
                                &config,
                                &memory,
                                &observer_state,
                            );
                            window_for_run.request_redraw();
                        }
                        "persona_tsundere" => {
                            config.persona = "tsundere".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            queue_contextual_behavior(
                                &mut pending_behavior,
                                BehaviorEvent::PersonaShift,
                                &config,
                                &memory,
                                &observer_state,
                            );
                            window_for_run.request_redraw();
                        }
                        "persona_toxic" => {
                            config.persona = "toxic".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            queue_contextual_behavior(
                                &mut pending_behavior,
                                BehaviorEvent::PersonaShift,
                                &config,
                                &memory,
                                &observer_state,
                            );
                            window_for_run.request_redraw();
                        }
                        "persona_moe" => {
                            config.persona = "moe".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            queue_contextual_behavior(
                                &mut pending_behavior,
                                BehaviorEvent::PersonaShift,
                                &config,
                                &memory,
                                &observer_state,
                            );
                            window_for_run.request_redraw();
                        }
                        "style_nimbus" => {
                            config.pet_style = "nimbus".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            pet_renderer.active_style = "nimbus".to_string();
                            bubble_renderer.active_style = "nimbus".to_string();
                            resize_pet_window(
                                &window_for_run,
                                &mut wgpu_state,
                                &mut pet_renderer,
                                config.pet_size,
                                &config.pet_style,
                                config.font_size,
                            );
                            queue_contextual_behavior(
                                &mut pending_behavior,
                                BehaviorEvent::BodyChanged,
                                &config,
                                &memory,
                                &observer_state,
                            );
                            window_for_run.request_redraw();
                        }
                        "style_mochi" => {
                            config.pet_style = "mochi".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            pet_renderer.active_style = "mochi".to_string();
                            bubble_renderer.active_style = "mochi".to_string();
                            resize_pet_window(
                                &window_for_run,
                                &mut wgpu_state,
                                &mut pet_renderer,
                                config.pet_size,
                                &config.pet_style,
                                config.font_size,
                            );
                            queue_contextual_behavior(
                                &mut pending_behavior,
                                BehaviorEvent::BodyChanged,
                                &config,
                                &memory,
                                &observer_state,
                            );
                            window_for_run.request_redraw();
                        }
                        "style_starlit" => {
                            config.pet_style = "starlit".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            pet_renderer.active_style = "starlit".to_string();
                            bubble_renderer.active_style = "starlit".to_string();
                            resize_pet_window(
                                &window_for_run,
                                &mut wgpu_state,
                                &mut pet_renderer,
                                config.pet_size,
                                &config.pet_style,
                                config.font_size,
                            );
                            queue_contextual_behavior(
                                &mut pending_behavior,
                                BehaviorEvent::BodyChanged,
                                &config,
                                &memory,
                                &observer_state,
                            );
                            window_for_run.request_redraw();
                        }
                        "style_crab" => {
                            config.pet_style = "crab".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            pet_renderer.active_style = "crab".to_string();
                            bubble_renderer.active_style = "crab".to_string();
                            resize_pet_window(
                                &window_for_run,
                                &mut wgpu_state,
                                &mut pet_renderer,
                                config.pet_size,
                                &config.pet_style,
                                config.font_size,
                            );
                            queue_contextual_behavior(
                                &mut pending_behavior,
                                BehaviorEvent::BodyChanged,
                                &config,
                                &memory,
                                &observer_state,
                            );
                            window_for_run.request_redraw();
                        }
                        "style_luma" => {
                            config.pet_style = "luma".to_string();
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            pet_renderer.active_style = "luma".to_string();
                            bubble_renderer.active_style = "luma".to_string();
                            resize_pet_window(
                                &window_for_run,
                                &mut wgpu_state,
                                &mut pet_renderer,
                                config.pet_size,
                                &config.pet_style,
                                config.font_size,
                            );
                            queue_contextual_behavior(
                                &mut pending_behavior,
                                BehaviorEvent::BodyChanged,
                                &config,
                                &memory,
                                &observer_state,
                            );
                            window_for_run.request_redraw();
                        }
                        "weather_sunny" => {
                            config.weather_mode = "sunny".to_string();
                            config.save();
                            pet_renderer.active_weather = "sunny".to_string();
                            bubble_renderer.active_weather = "sunny".to_string();
                            queue_contextual_behavior(
                                &mut pending_behavior,
                                BehaviorEvent::WeatherChanged,
                                &config,
                                &memory,
                                &observer_state,
                            );
                            window_for_run.request_redraw();
                        }
                        "weather_windy" => {
                            config.weather_mode = "windy".to_string();
                            config.save();
                            pet_renderer.active_weather = "windy".to_string();
                            bubble_renderer.active_weather = "windy".to_string();
                            queue_contextual_behavior(
                                &mut pending_behavior,
                                BehaviorEvent::WeatherChanged,
                                &config,
                                &memory,
                                &observer_state,
                            );
                            window_for_run.request_redraw();
                        }
                        "weather_rainy" => {
                            config.weather_mode = "rainy".to_string();
                            config.save();
                            pet_renderer.active_weather = "rainy".to_string();
                            bubble_renderer.active_weather = "rainy".to_string();
                            queue_contextual_behavior(
                                &mut pending_behavior,
                                BehaviorEvent::WeatherChanged,
                                &config,
                                &memory,
                                &observer_state,
                            );
                            window_for_run.request_redraw();
                        }
                        "weather_snowy" => {
                            config.weather_mode = "snowy".to_string();
                            config.save();
                            pet_renderer.active_weather = "snowy".to_string();
                            bubble_renderer.active_weather = "snowy".to_string();
                            queue_contextual_behavior(
                                &mut pending_behavior,
                                BehaviorEvent::WeatherChanged,
                                &config,
                                &memory,
                                &observer_state,
                            );
                            window_for_run.request_redraw();
                        }
                        "size_150" | "size_200" | "size_250" | "size_300" | "size_350" => {
                            let new_size = match id {
                                "size_150" => 150,
                                "size_200" => 200,
                                "size_250" => 250,
                                "size_300" => 300,
                                "size_350" => 350,
                                _ => 200,
                            };
                            config.pet_size = new_size;
                            config.save();

                            resize_pet_window(
                                &window_for_run,
                                &mut wgpu_state,
                                &mut pet_renderer,
                                new_size,
                                &config.pet_style,
                                config.font_size,
                            );

                            queue_contextual_behavior(
                                &mut pending_behavior,
                                BehaviorEvent::BodyChanged,
                                &config,
                                &memory,
                                &observer_state,
                            );
                            window_for_run.request_redraw();
                        }
                        "font_20" => {
                            config.font_size = 20.0;
                            config.save();
                            pet_renderer.set_font_size(20.0);
                            bubble_renderer.set_font_size(20.0);
                            window_for_run.request_redraw();
                        }
                        "font_24" => {
                            config.font_size = 24.0;
                            config.save();
                            pet_renderer.set_font_size(24.0);
                            bubble_renderer.set_font_size(24.0);
                            window_for_run.request_redraw();
                        }
                        "font_28" => {
                            config.font_size = 28.0;
                            config.save();
                            pet_renderer.set_font_size(28.0);
                            bubble_renderer.set_font_size(28.0);
                            window_for_run.request_redraw();
                        }
                        "font_32" => {
                            config.font_size = 32.0;
                            config.save();
                            pet_renderer.set_font_size(32.0);
                            bubble_renderer.set_font_size(32.0);
                            window_for_run.request_redraw();
                        }
                        "action_clear" => {
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            active_response.clear();
                            bubble_renderer.set_text("✨ 对话上下文已重置！");
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "sleep_30" => {
                            config.idle_offload_secs = 30;
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.set_idle_timeout(30);
                            }
                        }
                        "sleep_60" => {
                            config.idle_offload_secs = 60;
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.set_idle_timeout(60);
                            }
                        }
                        "sleep_120" => {
                            config.idle_offload_secs = 120;
                            config.save();
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.set_idle_timeout(120);
                            }
                        }
                        "action_exit" => {
                            println!("Exiting Desk Pet via Menu Bar.");
                            elwt.exit();
                            return;
                        }
                        _ => {}
                    }
                    tray_controls.sync(&config);
                }

                use mouse_position::mouse_position::Mouse;
                let mut global_pointer_on_pet = false;
                if let Mouse::Position { x, y } = Mouse::get_mouse_position() {
                    if last_global_pointer_position != Some((x, y)) {
                        last_global_pointer_position = Some((x, y));
                        last_global_pointer_motion = now;
                    }
                    if let Ok(win_pos) = window_for_run.outer_position() {
                        let local_x = x as f64 - win_pos.x as f64;
                        let local_y = y as f64 - win_pos.y as f64;
                        cursor_position = winit::dpi::PhysicalPosition::new(local_x, local_y);
                        set_pet_pointer_from_physical(
                            &mut pet_renderer,
                            window_for_run.inner_size(),
                            local_x,
                            local_y,
                        );
                        global_pointer_on_pet = pet_renderer
                            .body_hit_test(local_x, local_y)
                            .unwrap_or(false);
                    }
                }
                is_pointer_on_pet = global_pointer_on_pet;
                pet_renderer.pointer_inside = global_pointer_on_pet;

                if config.click_through_when_idle {
                    let desired_hittest = global_pointer_on_pet || is_dragging_pet;
                    if desired_hittest != pet_hittest_enabled {
                        if let Err(err) = window_for_run.set_cursor_hittest(desired_hittest) {
                            eprintln!("Failed to update pet hit testing: {err:?}");
                        } else {
                            pet_hittest_enabled = desired_hittest;
                        }
                    }
                }

                if global_pointer_on_pet && !was_pointer_on_pet {
                    let throttle = last_hover_trigger_time
                        .map(|last| {
                            now.duration_since(last)
                                > std::time::Duration::from_secs(
                                    config.touch_response_cooldown_secs,
                                )
                        })
                        .unwrap_or(true);
                    if throttle {
                        last_hover_trigger_time = Some(now);
                        last_interactive_time = now;
                        pet_renderer.add_emotion(0.18, 0.08);
                        if let Ok(mut mem) = memory.lock() {
                            mem.increment_touch();
                            mem.add_personality_note(touch_memory_note(&config.pet_style));
                        }
                        let (app, hour) = observer_state
                            .lock()
                            .map(|guard| guard.clone())
                            .unwrap_or_else(|_| ("未知应用".to_string(), 12));
                        let context = behavior_context(&config, &memory, app, hour);
                        if active_generation.is_none() {
                            if let Some(ref engine) = llama_engine {
                                active_response.clear();
                                filter = think_filter::ThinkBlockFilter::new(
                                    config.expose_think,
                                    false,
                                );
                                in_thinking_block = false;
                                spawn_behavior_generation(
                                    engine.clone(),
                                    tx.clone(),
                                    BehaviorEvent::Touch,
                                    context.clone(),
                                    false,
                                );
                                active_generation = Some(ActiveGeneration {
                                    kind: GenerationKind::Behavior(BehaviorEvent::Touch),
                                    context: Some(context),
                                    retry_count: 0,
                                    started_at: std::time::Instant::now(),
                                });
                            } else {
                                active_response = fallback_for_behavior(
                                    BehaviorEvent::Touch,
                                    &config.pet_style,
                                    &config.weather_mode,
                                    "未知应用",
                                    hour,
                                );
                                bubble_reveal = Some(TextReveal::new(
                                    active_response.clone(),
                                    now,
                                    BehaviorEvent::Touch.display_duration(),
                                ));
                            }
                        } else {
                            queue_behavior(
                                &mut pending_behavior,
                                PendingBehavior {
                                    event: BehaviorEvent::Touch,
                                    context,
                                },
                            );
                        }
                        let entropy = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos();
                        next_mutter_at = now
                            + jittered_interval(config.mutter_interval_secs, entropy);
                        next_dream_at = now
                            + jittered_interval(
                                config.dream_interval_secs,
                                entropy.rotate_left(17),
                            );
                    }
                }
                was_pointer_on_pet = global_pointer_on_pet;

                if active_generation.is_none() {
                    if let Some(request) = pending_behavior.take() {
                        if let Some(ref engine) = llama_engine {
                            active_response.clear();
                            filter = think_filter::ThinkBlockFilter::new(
                                config.expose_think,
                                false,
                            );
                            in_thinking_block = false;
                            spawn_behavior_generation(
                                engine.clone(),
                                tx.clone(),
                                request.event,
                                request.context.clone(),
                                false,
                            );
                            active_generation = Some(ActiveGeneration {
                                kind: GenerationKind::Behavior(request.event),
                                context: Some(request.context),
                                retry_count: 0,
                                started_at: std::time::Instant::now(),
                            });
                        } else {
                            active_response = fallback_for_behavior(
                                request.event,
                                &config.pet_style,
                                &request.context.weather,
                                &request.context.app,
                                request.context.hour,
                            );
                            bubble_reveal = Some(TextReveal::new(
                                active_response.clone(),
                                now,
                                request.event.display_duration(),
                            ));
                        }
                    }
                }

                let sleeping = state_machine.current() == PetState::Sleeping;
                if sleeping && !was_sleeping {
                    let entropy = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos();
                    next_dream_at = now
                        + jittered_interval(config.dream_interval_secs, entropy.rotate_left(17));
                }
                was_sleeping = sleeping;

                let ready_for_autonomy = active_generation.is_none()
                    && pending_behavior.is_none()
                    && bubble_reveal.is_none()
                    && bubble_timer.is_none();
                let due_event = if ready_for_autonomy && config.mutter_enabled && sleeping && now >= next_dream_at {
                    Some(BehaviorEvent::Dream)
                } else if ready_for_autonomy
                    && !sleeping
                    && config.mutter_enabled
                    && now >= next_mutter_at
                {
                    Some(BehaviorEvent::Ambient)
                } else {
                    None
                };

                if let Some(event) = due_event {
                    let (app, hour) = observer_state
                        .lock()
                        .map(|guard| guard.clone())
                        .unwrap_or_else(|_| ("未知应用".to_string(), 12));
                    let context = behavior_context(&config, &memory, app, hour);
                    if event == BehaviorEvent::Dream {
                        if let Ok(mut mem) = memory.lock() {
                            mem.increment_dreams();
                        }
                    }
                    if let Some(ref engine) = llama_engine {
                        active_response.clear();
                        filter = think_filter::ThinkBlockFilter::new(
                            config.expose_think,
                            false,
                        );
                        in_thinking_block = false;
                        spawn_behavior_generation(
                            engine.clone(),
                            tx.clone(),
                            event,
                            context.clone(),
                            false,
                        );
                        active_generation = Some(ActiveGeneration {
                            kind: GenerationKind::Behavior(event),
                            context: Some(context),
                            retry_count: 0,
                            started_at: std::time::Instant::now(),
                        });
                    } else {
                        active_response = fallback_for_behavior(
                            event,
                            &config.pet_style,
                            &context.weather,
                            &context.app,
                            context.hour,
                        );
                        bubble_reveal = Some(TextReveal::new(
                            active_response.clone(),
                            now,
                            event.display_duration(),
                        ));
                    }
                    let entropy = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos();
                    if event == BehaviorEvent::Dream {
                        next_dream_at = now
                            + jittered_interval(
                                config.dream_interval_secs,
                                entropy.rotate_left(17),
                            );
                    } else {
                        next_mutter_at = now
                            + jittered_interval(config.mutter_interval_secs, entropy);
                    }
                }

                let is_active = is_pointer_on_pet
                    || pet_renderer.pet_state == "thinking"
                    || pet_renderer.pet_state == "muttering"
                    || pet_renderer.pet_state == "surprised"
                    || active_generation.is_some()
                    || bubble_reveal.is_some()
                    || now.duration_since(last_state_change)
                        < std::time::Duration::from_millis(900)
                    || now.duration_since(last_global_pointer_motion)
                        < std::time::Duration::from_millis(450);
                let interval = if is_active {
                    std::time::Duration::from_millis(33)
                } else {
                    std::time::Duration::from_millis(125)
                };

                if now.duration_since(last_draw_time) >= interval {
                    window_for_run.request_redraw();
                    if bubble_reveal.is_some()
                        || active_generation.as_ref().is_some_and(|session| {
                            session.kind == GenerationKind::Chat
                        })
                    {
                        bubble_window_clone.request_redraw();
                    }
                }
                elwt.set_control_flow(ControlFlow::WaitUntil(now + interval));
            }
            _ => {}
        }
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_no_crab_identity_leak(text: &str) {
        assert!(
            !text.contains("云朵") && !text.contains("小云") && !text.contains("猫咪"),
            "crab text leaked another identity: {text}"
        );
    }

    #[test]
    fn crab_local_visible_text_stays_crab_shaped() {
        for app in ["Code", "Safari", "WeChat", "Finder"] {
            assert_no_crab_identity_leak(&get_offline_mutter(app, 14, "crab"));
        }
        assert_no_crab_identity_leak(&get_offline_mutter("Finder", 2, "crab"));
        assert_no_crab_identity_leak(startup_fallback_line("crab"));
        assert!(!startup_fallback_line("crab").contains("Nimbus"));
        assert_no_crab_identity_leak(style_switch_line("crab"));
        for line in touch_response_lines("crab") {
            assert_no_crab_identity_leak(line);
        }
        for weather in ["sunny", "windy", "rainy", "snowy"] {
            assert_no_crab_identity_leak(weather_feedback_line("crab", weather));
        }
    }

    #[test]
    fn crab_prompt_uses_positive_style_guidance() {
        let prompt = get_persona_prompt("default", "crab", "Nimbus", "sunny");

        assert!(prompt.contains("小螃蟹"));
        assert!(prompt.contains("大螯"));
        assert!(prompt.contains("过去造型"));
        assert!(!prompt.contains("Nimbus"));
        assert!(!prompt.contains("绝对不要自称"));
    }

    #[test]
    fn luma_local_fallbacks_and_prompt_keep_the_rabbit_identity() {
        let prompt = get_persona_prompt("default", "luma", "Nimbus", "snowy");
        assert!(prompt.contains("月灯兔"));
        assert!(prompt.contains("长耳朵"));
        assert!(!prompt.contains("Nimbus"));

        for line in [
            startup_fallback_line("luma"),
            style_switch_line("luma"),
            weather_feedback_line("luma", "sunny"),
            weather_feedback_line("luma", "windy"),
            weather_feedback_line("luma", "rainy"),
            weather_feedback_line("luma", "snowy"),
        ] {
            assert_no_crab_identity_leak_for_style(line, "luma");
        }
        for line in touch_response_lines("luma") {
            assert_no_crab_identity_leak_for_style(line, "luma");
        }
    }

    #[test]
    fn persona_prompt_suppresses_legacy_default_name() {
        let prompt = get_persona_prompt("default", "nimbus", "Nimbus", "sunny");

        assert!(!prompt.to_lowercase().contains("nimbus"));
        assert!(!prompt.contains("名字叫"));
    }

    #[test]
    fn persona_prompt_keeps_user_custom_name() {
        let prompt = get_persona_prompt("default", "crab", "小贝", "sunny");

        assert!(prompt.contains("名字叫小贝"));
        assert!(!prompt.contains("Nimbus"));
    }

    #[test]
    fn typewriter_reveal_advances_without_skipping_characters() {
        let start = std::time::Instant::now();
        let mut reveal = TextReveal::new(
            "螃蟹挥挥小螯".to_string(),
            start,
            std::time::Duration::from_secs(7),
        );

        assert_eq!(reveal.advance(start).as_deref(), Some("螃"));
        assert_eq!(
            reveal.advance(start + std::time::Duration::from_millis(10)),
            None
        );
        assert_eq!(
            reveal
                .advance(start + std::time::Duration::from_millis(50))
                .as_deref(),
            Some("螃蟹")
        );
    }

    #[test]
    fn queued_touch_replaces_lower_priority_ambient_behavior() {
        let context = BehaviorContext {
            pet_style: "crab".to_string(),
            persona: "default".to_string(),
            weather: "sunny".to_string(),
            app: "Finder".to_string(),
            hour: 12,
            memory_summary: String::new(),
            recent_lines: Vec::new(),
        };
        let mut pending = Some(PendingBehavior {
            event: BehaviorEvent::Ambient,
            context: context.clone(),
        });
        queue_behavior(
            &mut pending,
            PendingBehavior {
                event: BehaviorEvent::Touch,
                context,
            },
        );

        assert_eq!(pending.unwrap().event, BehaviorEvent::Touch);
    }

    #[test]
    fn pet_window_size_is_tight_but_preserves_nominal_visual_scale() {
        assert_eq!(pet_window_logical_size(150, "crab"), (132, 75));
        assert_eq!(pet_window_logical_size(300, "mochi"), (264, 150));
        assert_eq!(pet_window_logical_size(350, "crab"), (308, 175));
        assert_eq!(pet_window_logical_size(350, "nimbus"), (308, 189));
    }

    #[tokio::test]
    #[ignore = "requires the local MiniCPM model and Metal runtime"]
    async fn test_model_mutter_generation_robustness() {
        let model_path = std::path::PathBuf::from(
            "/Users/jinliang/.lmstudio/models/openbmb/MiniCPM5-1B-GGUF/MiniCPM5-1B-Q4_K_M.gguf",
        );
        if !model_path.exists() {
            println!("Skipping Llama model test: model file not found.");
            return;
        }

        let engine = LlamaEngine::new(model_path, 30).expect("Failed to init LlamaEngine");

        let styles = ["crab", "mochi", "starlit", "nimbus", "luma"];
        let weather_modes = ["sunny", "windy", "rainy", "snowy"];
        let personas = ["tsundere", "toxic", "moe", "default"];

        for (i, style) in styles.iter().enumerate() {
            let weather = weather_modes[i % weather_modes.len()];
            let persona = personas[i % personas.len()];
            let context = BehaviorContext {
                pet_style: (*style).to_string(),
                persona: persona.to_string(),
                weather: weather.to_string(),
                app: "Code".to_string(),
                hour: 14,
                memory_summary: "对方正在写代码，最近喜欢安静陪伴。".to_string(),
                recent_lines: Vec::new(),
            };
            let prompt = build_behavior_prompt(BehaviorEvent::Ambient, &context, false);

            let mut stream = engine
                .generate_mutter_stream(&prompt)
                .await
                .expect("Failed to generate mutter");
            let mut response = String::new();
            while let Some(chunk) = stream.next().await {
                response.push_str(&chunk);
            }

            let cleaned = clean_behavior_line(&response, BehaviorEvent::Ambient, style, 14, &[])
                .unwrap_or_else(|| get_offline_mutter("Code", 14, style));
            assert!(!cleaned.is_empty(), "Mutter should not be empty");
            assert_no_crab_identity_leak_for_style(&cleaned, style);
        }
    }

    fn assert_no_crab_identity_leak_for_style(text: &str, style: &str) {
        let t = text.to_lowercase();
        match style {
            "crab" => {
                assert!(
                    !t.contains("云")
                        && !t.contains("猫")
                        && !t.contains("喵")
                        && !t.contains("章鱼"),
                    "crab leaked: {text}"
                );
            }
            "mochi" => {
                assert!(
                    !t.contains("螃蟹")
                        && !t.contains("螯")
                        && !t.contains("云")
                        && !t.contains("章鱼"),
                    "mochi leaked: {text}"
                );
            }
            "starlit" => {
                assert!(
                    !t.contains("螃蟹")
                        && !t.contains("螯")
                        && !t.contains("猫")
                        && !t.contains("喵")
                        && !t.contains("云"),
                    "starlit leaked: {text}"
                );
            }
            "luma" => {
                assert!(
                    !t.contains("螃蟹")
                        && !t.contains("螯")
                        && !t.contains("猫")
                        && !t.contains("喵")
                        && !t.contains("章鱼")
                        && !t.contains("云"),
                    "luma leaked: {text}"
                );
            }
            _ => {
                assert!(
                    !t.contains("螃蟹")
                        && !t.contains("螯")
                        && !t.contains("猫")
                        && !t.contains("喵")
                        && !t.contains("章鱼"),
                    "nimbus leaked: {text}"
                );
            }
        }
    }
}
