use std::sync::Arc;
use tokio::sync::mpsc;
use winit::{
    event::{Event, WindowEvent, ElementState, MouseButton},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use futures_util::StreamExt;

mod config;
mod memory;
mod think_filter;
mod llama_engine;
mod renderer;

use config::AppConfig;
use llama_engine::LlamaEngine;
use memory::PetMemory;
use renderer::PetRenderer;
use tray_icon::{
    menu::{Menu, MenuItem, Submenu, PredefinedMenuItem, CheckMenuItem, MenuEvent},
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
    async fn new(window: Arc<winit::window::Window>, width: u32, height: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        
        let surface = instance.create_surface(window.clone())?;
        
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).await.ok_or("Failed to find an appropriate adapter")?;
        
        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
            },
            None,
        ).await?;
        
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps.formats.iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
            
        let alpha_mode = if surface_caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else if surface_caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::PostMultiplied) {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else {
            wgpu::CompositeAlphaMode::Opaque
        };

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
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
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(r#"
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
            "#)),
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
        
        let render_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&self.device, &self.config);
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
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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

// User Activity Observer: Runs AppleScript to fetch frontmost app name and current hour
async fn observe_user_activity() -> (String, u32) {
    let output = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to get name of first process whose frontmost is true")
        .arg("-e")
        .arg("get hours of (current date)")
        .output()
        .await;

    match output {
        Ok(out) => {
            let combined = String::from_utf8_lossy(&out.stdout).to_string();
            let mut lines = combined.lines();
            let app_name = lines.next().unwrap_or("未知应用").trim().to_string();
            let hour = lines.next()
                .unwrap_or("12")
                .trim()
                .parse::<u32>()
                .unwrap_or(12);
            (app_name, hour)
        }
        Err(_) => ("未知应用".to_string(), 12),
    }
}

// Pseudo-random index selector without rand dependency
fn get_pseudo_random_index(max: usize) -> usize {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    (now % max as u128) as usize
}

// Offline muttering template based on active app and local time
fn get_offline_mutter(app: &str, hour: u32) -> String {
    if hour >= 23 || hour <= 5 {
        return "夜深啦，小云朵已经有点困了，主人也要早点休息哦 (￣o￣).zZ".to_string();
    }
    
    let app_lower = app.to_lowercase();
    if app_lower.contains("code") || app_lower.contains("terminal") || app_lower.contains("xcode") || app_lower.contains("cargo") || app_lower.contains("idea") {
        return "主人一直在认真写代码呢，小云朵在陪着你，喝杯温水歇歇吧~".to_string();
    }
    if app_lower.contains("safari") || app_lower.contains("chrome") || app_lower.contains("firefox") || app_lower.contains("browser") {
        return "网页刷个不停，小云朵帮你吹吹风，歇一会眼睛吧~".to_string();
    }
    if app_lower.contains("chat") || app_lower.contains("wechat") || app_lower.contains("slack") || app_lower.contains("discord") || app_lower.contains("ding") {
        return "在和谁聊天呢？小云朵也想加入，嘿嘿~".to_string();
    }
    
    let generic = [
        "云朵今天也想飞得更高呢~",
        "主人敲代码累了吗？云朵帮你捏捏肩（虽然没有手）~",
        "偷偷看一眼主人在干嘛...",
        "云朵在空气中漂浮着，呼呼...",
        "又是和主人在一起的一天，真开心！",
        "主人加油！你是最棒的！(*^▽^*)",
        "小云朵打了个滚，变成了一片甜甜的棉花糖~"
    ];
    
    let idx = get_pseudo_random_index(generic.len());
    generic[idx].to_string()
}

fn create_tray_icon() -> tray_icon::Icon {
    let mut rgba = vec![0u8; 16 * 16 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let idx = (y * 16 + x) * 4;
            // Draw a cute cloud shape
            let dx = x as f32 - 8.0;
            let dy = y as f32 - 8.0;
            let dist = (dx*dx + dy*dy).sqrt();
            if dist < 5.0 || (y >= 7 && y <= 11 && x >= 3 && x <= 13) {
                rgba[idx] = 135;
                rgba[idx+1] = 206;
                rgba[idx+2] = 250;
                rgba[idx+3] = 255;
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, 16, 16).unwrap()
}

fn get_persona_prompt(persona: &str, pet_name: &str) -> String {
    match persona {
        "tsundere" => format!("你是一只可爱的傲娇桌宠{}，说话喜欢带'哼'，明明很关心主人却嘴硬，傲娇且别扭。", pet_name),
        "toxic" => format!("你是一只可爱的毒舌桌宠{}，喜欢一针见血地吐槽主人的日常，带一点幽默的冷酷和嫌弃。", pet_name),
        "moe" => format!("你是一只极其软萌粘人、充满治愈感的桌宠{}，把主人当成世界上最喜欢的人，说话超甜、极其粘人，多用‘~’和叠词。", pet_name),
        _ => format!("你是一只可爱的桌宠{}，性格软萌活泼，说话温柔体贴，喜欢和主人贴贴。", pet_name),
    }
}

fn prompt_for_pet_name(current_name: &str) -> Option<String> {
    let script = format!(
        "display dialog \"请输入桌宠的新名字:\" default answer \"{}\" with title \"设置桌宠名字\" with icon note",
        current_name
    );
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .ok()?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(pos) = stdout.find("text returned:") {
            let name = stdout[pos + "text returned:".len()..].trim().to_string();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

fn show_system_dialog(title: &str, message: &str) {
    let script = format!(
        "display dialog \"{}\" with title \"{}\" buttons {{\"好的\"}} default button \"好的\" with icon note",
        message, title
    );
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .status();
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
    let _tray_icon = {
        let menu = Menu::new();
        
        let mutter_check = CheckMenuItem::with_id("mutter_enabled", "开启自言自语", true, config.mutter_enabled, None);
        menu.append(&mutter_check).unwrap();
        
        let persona_submenu = Submenu::with_id("persona", "切换桌宠人设", true);
        let p_default = MenuItem::with_id("persona_default", "软萌活泼 (默认)", true, None);
        let p_tsundere = MenuItem::with_id("persona_tsundere", "傲娇嘴硬", true, None);
        let p_toxic = MenuItem::with_id("persona_toxic", "冷酷毒舌", true, None);
        let p_moe = MenuItem::with_id("persona_moe", "极度粘人", true, None);
        persona_submenu.append_items(&[&p_default, &p_tsundere, &p_toxic, &p_moe]).unwrap();
        menu.append(&persona_submenu).unwrap();
        
        let font_submenu = Submenu::with_id("font_size", "调整字号大小", true);
        let f_20 = MenuItem::with_id("font_20", "20 像素", true, None);
        let f_24 = MenuItem::with_id("font_24", "24 像素 (默认)", true, None);
        let f_28 = MenuItem::with_id("font_28", "28 像素", true, None);
        let f_32 = MenuItem::with_id("font_32", "32 像素", true, None);
        font_submenu.append_items(&[&f_20, &f_24, &f_28, &f_32]).unwrap();
        menu.append(&font_submenu).unwrap();

        let config_submenu = Submenu::with_id("config_sub", "系统参数设置", true);
        let c_clear = MenuItem::with_id("action_clear", "清空对话历史", true, None);
        let c_sleep_30 = MenuItem::with_id("sleep_30", "显存休眠 30 秒", true, None);
        let c_sleep_60 = MenuItem::with_id("sleep_60", "显存休眠 60 秒", true, None);
        let c_sleep_120 = MenuItem::with_id("sleep_120", "显存休眠 120 秒", true, None);
        config_submenu.append_items(&[&c_clear, &c_sleep_30, &c_sleep_60, &c_sleep_120]).unwrap();
        menu.append(&config_submenu).unwrap();
        
        let rename_item = MenuItem::with_id("action_rename", "🖊️ 修改桌宠名字", true, None);
        menu.append(&rename_item).unwrap();

        let memory_submenu = Submenu::with_id("memory_sub", "💾 主人记忆管理", true);
        let m_show = MenuItem::with_id("memory_show_info", "查看记忆详情", true, None);
        let m_open = MenuItem::with_id("memory_open_dir", "打开记忆目录", true, None);
        let m_clear = MenuItem::with_id("memory_clear", "清空所有记忆", true, None);
        memory_submenu.append_items(&[&m_show, &m_open, &m_clear]).unwrap();
        menu.append(&memory_submenu).unwrap();

        menu.append(&PredefinedMenuItem::separator()).unwrap();
        let exit_item = MenuItem::with_id("action_exit", "退出桌宠", true, None);
        menu.append(&exit_item).unwrap();

        TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("灵霓 Nimbus 桌宠控制面板")
            .with_icon(create_tray_icon())
            .build()
            .unwrap()
    };
    
    // Enlarged window dimension for premium aesthetics
    let win_width = 360;
    let win_height = 440;

    // 3. Setup Window
    let window = WindowBuilder::new()
        .with_title("Nimbus Desk Pet Widget")
        .with_decorations(false)           // Borderless
        .with_transparent(true)             // Transparent background
        .with_resizable(false)              // Lock size
        .with_window_level(if config.always_on_top {
            winit::window::WindowLevel::AlwaysOnTop
        } else {
            winit::window::WindowLevel::Normal
        })
        .with_inner_size(winit::dpi::PhysicalSize::new(win_width, win_height))
        .build(&event_loop)?;

    // Position window in bottom right corner by default
    if let Some(monitor) = window.current_monitor() {
        let screen_size = monitor.size();
        let x = screen_size.width - win_width - 50;
        let y = screen_size.height - win_height - 100;
        window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
    }

    let window = Arc::new(window);
    let window_clone = window.clone();

    // Spawn Background Frame redraw trigger (30 FPS)
    let window_for_redraw = window_clone.clone();
    tokio::spawn(async move {
        loop {
            window_for_redraw.request_redraw();
            tokio::time::sleep(std::time::Duration::from_millis(33)).await;
        }
    });

    // 3. Initialize Wgpu State
    let mut wgpu_state = WgpuState::new(window.clone(), win_width, win_height).await?;

    // 4. Initialize custom 2D Renderer and state
    let mut pet_renderer = PetRenderer::new(win_width, win_height);
    pet_renderer.set_font_size(config.font_size);
    pet_renderer.set_text(&format!("你好呀！我是桌宠{}！\n请在终端打字和我聊天吧~", config.pet_name));

    // 5. Initialize in-process LlamaEngine with Smart Idle Offload config
    let llama_engine = if let Some(model_path) = &config.model_path {
        match LlamaEngine::new(model_path.clone(), config.idle_offload_secs) {
            Ok(engine) => {
                println!("LlamaEngine initialized successfully (In-Process mode)!");
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
    let mut was_online = false;
    let mut is_pointer_on_pet = false;
    let mut surprised_timer: Option<std::time::Instant> = None;
    let mut is_muttering = false;
    let mut last_mouse_phys_x = 0.0f64;
    let mut last_mouse_phys_y = 0.0f64;
    
    // Channels for text streaming and terminal input
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(100);
    let mut active_response = String::new();

    // 6. Spawn Stdin non-blocking reader task
    let idle_sec_hint = config.idle_offload_secs;
    let pet_name_clone = config.pet_name.clone();
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let mut reader = BufReader::new(tokio::io::stdin());
        let mut line = String::new();
        
        println!("==================================================");
        println!("      桌宠 {} 极速推理与资源优化版       ", pet_name_clone);
        println!("==================================================");
        println!("* 一体化单进程推理已加载。");
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
    let mut bubble_timer: Option<std::time::Instant> = Some(std::time::Instant::now()); // Initial hint
    let mut bubble_show_duration = std::time::Duration::from_secs(config.bubble_timeout_secs);

    // Thinking Block Stream Filter states
    let mut filter = think_filter::ThinkBlockFilter::new(config.expose_think, false);
    let mut in_thinking_block = false;

    // Trigger initial redraw
    window_clone.request_redraw();

    let window_for_run = window_clone.clone();

    // 7. Run Event Loop
    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Poll);

        // Detect LLM Offload/Restore status changes
        let is_online = if let Some(ref engine) = llama_engine {
            engine.is_model_online()
        } else {
            false
        };
        
        if was_online && !is_online {
            // Smart Offload Timeout Triggered (Model exited VRAM)
            pet_renderer.pet_state = "sleeping".to_string();
            pet_renderer.set_text("显存已释放，我先去打个盹啦，呼...");
            bubble_timer = Some(std::time::Instant::now());
            bubble_show_duration = std::time::Duration::from_secs(8);
            last_interactive_time = std::time::Instant::now();
            window_for_run.request_redraw();
        } else if !was_online && is_online {
            // Model re-activated
            if pet_renderer.pet_state == "sleeping" {
                pet_renderer.pet_state = "idle".to_string();
                window_for_run.request_redraw();
            }
        }
        was_online = is_online;

        // Process incoming Stdin commands
        while let Ok(input_prompt) = stdin_rx.try_recv() {
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
                pet_renderer.set_text("✨ 对话上下文已重置！");
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
                pet_renderer.pet_state = "thinking".to_string();
                pet_renderer.set_text("思考中...");
                bubble_timer = None; // Disable hide timer during generation
                last_interactive_time = std::time::Instant::now();
                window_for_run.request_redraw();

                if let Ok(mut mem) = memory.lock() {
                    mem.increment_interactions();
                }

                let memory_desc = {
                    if let Ok(mem) = memory.lock() {
                        let top_apps = mem.get_top_apps();
                        let top_hours = mem.get_top_hours();
                        if top_apps.is_empty() {
                            "你还没有关于主人的日常偏好记忆。".to_string()
                        } else {
                            format!(
                                "你记得主人的日常偏好：主人最常打开的应用是 [{}]，通常在 [{}] 点最活跃。",
                                top_apps.join(", "),
                                top_hours.iter().map(|h| format!("{}点", h)).collect::<Vec<_>>().join("/")
                            )
                        }
                    } else {
                        "你还没有关于主人的日常偏好记忆。".to_string()
                    }
                };

                let persona_prompt = get_persona_prompt(&config.persona, &config.pet_name);
                let prompt = format!(
                    "人设设定：{}。{}。主人对你说：\"{}\"。",
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
            } else {
                active_response = "本地模型未加载！\n请在 config.toml 中配置 model_path。".to_string();
                pet_renderer.set_text(&active_response);
                bubble_timer = Some(std::time::Instant::now());
                bubble_show_duration = std::time::Duration::from_secs(6);
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
                pet_renderer.set_text(&active_response);
                is_muttering = false;
                
                println!();
                
                bubble_timer = Some(std::time::Instant::now());
                bubble_show_duration = std::time::Duration::from_secs(config.bubble_timeout_secs);
                last_interactive_time = std::time::Instant::now();
                window_for_run.request_redraw();
            } else if new_text == "[OFFLINE]" {
                // Mutter fallback triggered if model is currently offline (handled below in muttering dispatch)
                let (app, hour) = {
                    if let Ok(guard) = observer_state.lock() {
                        guard.clone()
                    } else {
                        ("未知应用".to_string(), 12)
                    }
                };
                let mutter = get_offline_mutter(&app, hour);
                pet_renderer.set_text(&mutter);
                is_muttering = false;
                bubble_timer = Some(std::time::Instant::now());
                bubble_show_duration = std::time::Duration::from_secs(8);
                last_interactive_time = std::time::Instant::now();
                window_for_run.request_redraw();
            } else {
                print!("{}", new_text);
                use std::io::Write;
                let _ = std::io::stdout().flush();
                is_muttering = true;

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
                pet_renderer.set_text(&active_response);
                window_for_run.request_redraw();
            }
        }

        // Handle auto-fade timeout for chat bubble
        if let Some(timer) = bubble_timer {
            if std::time::Instant::now().duration_since(timer) > bubble_show_duration {
                active_response.clear();
                pet_renderer.set_text("");
                bubble_timer = None;
                // If it was sleeping, keep the sleep animation but hide bubble
                window_for_run.request_redraw();
            }
        }

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    elwt.exit();
                }
                WindowEvent::Resized(size) => {
                    wgpu_state.resize(size.width, size.height);
                    window_for_run.request_redraw();
                }
                WindowEvent::RedrawRequested => {
                    if !pet_renderer.pointer_inside {
                        use mouse_position::mouse_position::Mouse;
                        if let Mouse::Position { x, y } = Mouse::get_mouse_position() {
                            if let Ok(win_pos) = window_for_run.outer_position() {
                                let scale_factor = window_for_run.scale_factor();
                                let px = x as f64 * scale_factor;
                                let py = y as f64 * scale_factor;
                                let wx = px - win_pos.x as f64;
                                let wy = py - win_pos.y as f64;
                                pet_renderer.set_mouse_position(Some((wx as f32, wy as f32)));
                                pet_renderer.pointer_x = wx as f32 / (win_width as f32 / 24.0);
                                pet_renderer.pointer_y = wy as f32 / (win_height as f32 / 24.0);
                            }
                        }
                    }

                    // 智能状态决策引擎 (Smart State Decision Engine)
                    let current_state = {
                        let now_inst = std::time::Instant::now();
                        let is_surprised = if let Some(st) = surprised_timer {
                            now_inst.duration_since(st) < std::time::Duration::from_millis(1500)
                        } else {
                            false
                        };

                        if is_surprised {
                            "surprised".to_string()
                        } else if is_pointer_on_pet {
                            "tickled".to_string()
                        } else if is_muttering {
                            if in_thinking_block {
                                "thinking".to_string()
                            } else {
                                "muttering".to_string()
                            }
                        } else if pet_renderer.pointer_inside {
                            "attentive".to_string()
                        } else if !is_online {
                            "sleeping".to_string()
                        } else {
                            // 场景关联：陪伴工作 (working) / 深夜疲劳 (tired) / 挂机发呆 (bored) / 常规待机 (idle)
                            let (app, hour) = {
                                if let Ok(guard) = observer_state.lock() {
                                    guard.clone()
                                } else {
                                    ("未知应用".to_string(), 12)
                                }
                            };
                            let idle_duration = now_inst.duration_since(last_interactive_time);
                            
                            if idle_duration > std::time::Duration::from_secs(90) {
                                "bored".to_string()
                            } else {
                                let app_lower = app.to_lowercase();
                                let is_dev_app = app_lower.contains("code") 
                                    || app_lower.contains("terminal") 
                                    || app_lower.contains("xcode") 
                                    || app_lower.contains("cargo") 
                                    || app_lower.contains("idea")
                                    || app_lower.contains("warp");
                                    
                                if is_dev_app {
                                    "working".to_string()
                                } else if hour >= 23 || hour <= 5 {
                                    "tired".to_string()
                                } else {
                                    "idle".to_string()
                                }
                            }
                        }
                    };
                    pet_renderer.pet_state = current_state;

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
                        // Only allow drag if clicking directly on the enlarged pet body (cloud/cat/octopus pixels)
                        if is_on_pet {
                            if let Err(e) = window_for_run.drag_window() {
                                eprintln!("Failed to drag window: {:?}", e);
                            }
                        }
                    } else if state == ElementState::Released {
                        // 拖拽窗口松手或点击松开瞬间触发受惊表情，持续 1.5 秒
                        surprised_timer = Some(std::time::Instant::now());
                        pet_renderer.add_emotion(-0.02, 0.15);
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    pet_renderer.pointer_inside = true;
                    cursor_position = position;
                    last_interactive_time = std::time::Instant::now(); // 刷新活跃时间，即时唤醒挂机
                    
                    pet_renderer.set_mouse_position(Some((position.x as f32, position.y as f32)));
                    
                    pet_renderer.pointer_x = position.x as f32 / (win_width as f32 / 24.0);
                    pet_renderer.pointer_y = position.y as f32 / (win_height as f32 / 24.0);
                    
                    // 物理移速差分计算 (惊讶状态触发器)
                    let scale_factor = window_for_run.scale_factor();
                    let phys_x = position.x * scale_factor;
                    let phys_y = position.y * scale_factor;
                    if last_mouse_phys_x > 0.0 && last_mouse_phys_y > 0.0 {
                        let dx = phys_x - last_mouse_phys_x;
                        let dy = phys_y - last_mouse_phys_y;
                        let speed = (dx * dx + dy * dy).sqrt();
                        if speed > 220.0 {
                            surprised_timer = Some(std::time::Instant::now());
                            pet_renderer.add_emotion(-0.08, 0.35);
                        }
                    }
                    last_mouse_phys_x = phys_x;
                    last_mouse_phys_y = phys_y;

                    // Reactive behavior: Hover check using pixel mask (Touch the cloud/cat/octopus)
                    let is_on_pet = pet_renderer.body_hit_test(position.x, position.y).unwrap_or(false);
                    is_pointer_on_pet = is_on_pet;
                    
                    if is_on_pet && !was_pointer_on_pet {
                        // Just hovered onto pet!
                        let now = std::time::Instant::now();
                        let throttle = match last_hover_trigger_time {
                            Some(t) => now.duration_since(t) > std::time::Duration::from_secs(15),
                            None => true,
                        };
                        
                        if throttle && pet_renderer.pet_state != "thinking" {
                            last_hover_trigger_time = Some(now);
                            last_interactive_time = now;
                            
                            // Touch increases happiness and excitement
                            pet_renderer.add_emotion(0.18, 0.08);
                            
                            // Blush and speak cute words
                            let blush_phrases = [
                                "哎呀，摸到我啦~ (*^▽^*)",
                                "好痒呀，主人在跟我玩嘛？",
                                "贴贴！(*/ω＼*)",
                                "小云朵被摸得很舒服，呼呼..."
                            ];
                            let idx = get_pseudo_random_index(blush_phrases.len());
                            pet_renderer.set_text(blush_phrases[idx]);
                            
                            bubble_timer = Some(now);
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                    }
                    was_pointer_on_pet = is_on_pet;
                    window_for_run.request_redraw();
                }
                WindowEvent::CursorLeft { .. } => {
                    pet_renderer.pointer_inside = false;
                    is_pointer_on_pet = false;
                    window_for_run.request_redraw();
                }
                _ => {}
            },
            Event::AboutToWait => {
                let now = std::time::Instant::now();
                
                // Handle System Tray/Menubar events
                while let Ok(event) = MenuEvent::receiver().try_recv() {
                    let id = event.id.0.as_str();
                    match id {
                        "mutter_enabled" => {
                            config.mutter_enabled = !config.mutter_enabled;
                            config.save();
                        }
                        "action_rename" => {
                            if let Some(new_name) = prompt_for_pet_name(&config.pet_name) {
                                config.pet_name = new_name.clone();
                                config.save();
                                pet_renderer.set_text(&format!("✨ 从现在开始我就叫 [{}] 啦！", config.pet_name));
                                bubble_timer = Some(std::time::Instant::now());
                                bubble_show_duration = std::time::Duration::from_secs(5);
                                window_for_run.request_redraw();
                                println!("\n>> 桌宠重命名为：{}", new_name);
                            }
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
                                    format!("记忆文件：~/.nimbus_memory.json\\n文件大小：{}\\n互动次数：{}次\\n\\n【日常习惯习惯记忆】\\n{}", 
                                        size_str, mem.total_interactions, active_desc)
                                } else {
                                    "无法读取记忆。".to_string()
                                }
                            };
                            show_system_dialog("主人记忆日记详情", &info_msg);
                        }
                        "memory_open_dir" => {
                            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                            let _ = std::process::Command::new("open").arg(&home).status();
                        }
                        "memory_clear" => {
                            if let Ok(mut mem) = memory.lock() {
                                mem.clear();
                                pet_renderer.set_text("✨ 主人，我的记忆已经全部清空啦！");
                                bubble_timer = Some(std::time::Instant::now());
                                bubble_show_duration = std::time::Duration::from_secs(5);
                                window_for_run.request_redraw();
                                println!("\n>> 主人习惯记忆已重置。");
                            }
                        }
                        "persona_default" => {
                            config.persona = "default".to_string();
                            config.save();
                            pet_renderer.set_text(&format!("✨ 我现在是软萌活泼的{}哦~", config.pet_name));
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "persona_tsundere" => {
                            config.persona = "tsundere".to_string();
                            config.save();
                            pet_renderer.set_text("✨ 哼，才不是想让你陪我呢！");
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "persona_toxic" => {
                            config.persona = "toxic".to_string();
                            config.save();
                            pet_renderer.set_text("✨ 主人，今天的工作完成了吗？");
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "persona_moe" => {
                            config.persona = "moe".to_string();
                            config.save();
                            pet_renderer.set_text(&format!("✨ 亲爱的，{}最喜欢你啦~", config.pet_name));
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "font_20" => {
                            config.font_size = 20.0;
                            config.save();
                            pet_renderer.set_font_size(20.0);
                            window_for_run.request_redraw();
                        }
                        "font_24" => {
                            config.font_size = 24.0;
                            config.save();
                            pet_renderer.set_font_size(24.0);
                            window_for_run.request_redraw();
                        }
                        "font_28" => {
                            config.font_size = 28.0;
                            config.save();
                            pet_renderer.set_font_size(28.0);
                            window_for_run.request_redraw();
                        }
                        "font_32" => {
                            config.font_size = 32.0;
                            config.save();
                            pet_renderer.set_font_size(32.0);
                            window_for_run.request_redraw();
                        }
                        "action_clear" => {
                            if let Some(ref engine) = llama_engine {
                                let _ = engine.clear_history();
                            }
                            active_response.clear();
                            pet_renderer.set_text("✨ 对话上下文已重置！");
                            bubble_timer = Some(std::time::Instant::now());
                            bubble_show_duration = std::time::Duration::from_secs(5);
                            window_for_run.request_redraw();
                        }
                        "sleep_30" => {
                            config.idle_offload_secs = 30;
                            config.save();
                        }
                        "sleep_60" => {
                            config.idle_offload_secs = 60;
                            config.save();
                        }
                        "sleep_120" => {
                            config.idle_offload_secs = 120;
                            config.save();
                        }
                        "action_exit" => {
                            println!("Exiting Desk Pet via Menu Bar.");
                            elwt.exit();
                            return;
                        }
                        _ => {}
                    }
                }
                
                // Periodic Muttering: Check if idle for > 3 minutes (180 seconds)
                if config.mutter_enabled && now.duration_since(last_interactive_time) > std::time::Duration::from_secs(180) && bubble_timer.is_none() {
                    last_interactive_time = now;
                    
                    if let Some(ref engine) = llama_engine {
                        if engine.is_model_online() {
                            // Smart model-driven muttering: Query LLM with current app Context
                            let (app, hour) = {
                                if let Ok(guard) = observer_state.lock() {
                                    guard.clone()
                                } else {
                                    ("未知应用".to_string(), 12)
                                }
                            };
                            let memory_desc = {
                                if let Ok(mem) = memory.lock() {
                                    let top_apps = mem.get_top_apps();
                                    let top_hours = mem.get_top_hours();
                                    if top_apps.is_empty() {
                                        "你还没有关于主人的日常偏好记忆。".to_string()
                                    } else {
                                        format!(
                                            "你记得主人的日常偏好：主人最常打开的应用是 [{}]，通常在 [{}] 点最活跃。",
                                            top_apps.join(", "),
                                            top_hours.iter().map(|h| format!("{}点", h)).collect::<Vec<_>>().join("/")
                                        )
                                    }
                                } else {
                                    "你还没有关于主人的日常偏好记忆。".to_string()
                                }
                            };

                            let persona_prompt = get_persona_prompt(&config.persona, &config.pet_name);
                            let prompt = format!(
                                "人设设定：{}。{}。你的主人当前正处于前台应用：{}。现在时间小时数是：{}。你是一只可爱的桌宠{}，你在无聊地自言自语。请配合这个应用或时间以及你的记忆和人设语气自发说一句话，不要带任何格式，不超过二十个字。",
                                persona_prompt, memory_desc, app, hour, config.pet_name
                            );
                            
                            let engine_clone = engine.clone();
                            let tx_clone = tx.clone();
                            
                            tokio::spawn(async move {
                                if let Ok(mut stream) = engine_clone.generate_mutter_stream(&prompt).await {
                                    while let Some(chunk) = stream.next().await {
                                        let _ = tx_clone.send(chunk).await;
                                    }
                                    let _ = tx_clone.send("[DONE]".to_string()).await;
                                } else {
                                    let _ = tx_clone.send("[OFFLINE]".to_string()).await;
                                }
                            });
                        } else {
                            // Model offline offloaded: Fallback to lightweight template muttering
                            let (app, hour) = {
                                if let Ok(guard) = observer_state.lock() {
                                    guard.clone()
                                } else {
                                    ("未知应用".to_string(), 12)
                                }
                            };
                            let mutter = get_offline_mutter(&app, hour);
                            pet_renderer.set_text(&mutter);
                            bubble_timer = Some(now);
                            bubble_show_duration = std::time::Duration::from_secs(8);
                            window_for_run.request_redraw();
                        }
                    }
                }
                
                let is_active = pet_renderer.pointer_inside 
                    || pet_renderer.pet_state == "thinking" 
                    || bubble_timer.is_some();
                let interval = if is_active {
                    std::time::Duration::from_millis(16)
                } else {
                    std::time::Duration::from_millis(83)
                };
                
                if now.duration_since(last_draw_time) >= interval {
                    window_for_run.request_redraw();
                }
            }
            _ => {}
        }
    })?;

    Ok(())
}
