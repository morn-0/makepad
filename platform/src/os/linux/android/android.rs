#[allow(unused)]
use {
    std::rc::Rc,
    std::cell::{RefCell},
    std::ffi::CString,
    std::os::raw::{c_void},
    std::time::{Instant, Duration},
    std::sync::mpsc,
    std::collections::HashMap,
    self::super::{
        android_media::CxAndroidMedia,
        android_decoding::CxAndroidDecoding,
        jni_sys::jobject,
        android_jni::{self, *},
        android_keycodes::android_to_makepad_key_code,
        super::egl_sys::{self, LibEgl},
        ndk_sys,
    },
    self::super::super::{
        gl_sys,
        //libc_sys,
    },
    crate::{
        cx_api::{CxOsOp, CxOsApi},
        makepad_math::*,
        makepad_live_id::*,
        makepad_live_compiler::LiveFileChange,
        thread::Signal,
        event::{
            VirtualKeyboardEvent,
            NetworkResponseEvent,
            NetworkResponse,
            HttpResponse,
            TouchPoint,
            TouchUpdateEvent,
            WindowGeomChangeEvent,
            TimerEvent,
            TextInputEvent,
            TextClipboardEvent,
            KeyEvent,
            KeyModifiers,
            KeyCode,
            Event,
            WindowGeom,
            VideoDecodingInitializedEvent,
            VideoColorFormat,
            VideoStreamEvent,
            VideoDecodingErrorEvent,
            HttpRequest,
            HttpMethod,
        },
        window::CxWindowPool,
        pass::CxPassParent,
        cx::{Cx, OsType, AndroidParams},
        gpu_info::GpuPerformance,
        os::cx_native::EventFlow,
        pass::{PassClearColor, PassClearDepth, PassId},
    }
};

impl Cx {
    pub fn main_loop(&mut self, from_java_rx: mpsc::Receiver<FromJavaMessage>) {
        
        //elf.android_load_dependencies();
        self.gpu_info.performance = GpuPerformance::Tier1;
        
        self.call_event_handler(&Event::Construct);
        self.redraw_all();
        
        self.start_network_live_file_watcher();
        
        while !self.os.quit {
            self.handle_timers();
            
            while let Ok(msg) = from_java_rx.try_recv() {
                match msg {
                    FromJavaMessage::SurfaceCreated {window} => unsafe {
                        self.os.display.as_mut().unwrap().update_surface(window);
                    },
                    FromJavaMessage::SurfaceDestroyed => unsafe {
                        self.os.display.as_mut().unwrap().destroy_surface();
                    },
                    FromJavaMessage::SurfaceChanged {
                        window,
                        width,
                        height,
                    } => {
                        
                        unsafe {
                            self.os.display.as_mut().unwrap().update_surface(window);
                        }
                        self.os.display_size = dvec2(width as f64, height as f64);
                        let window_id = CxWindowPool::id_zero();
                        let window = &mut self.windows[window_id];
                        let old_geom = window.window_geom.clone();
                        let dpi_factor = window.dpi_override.unwrap_or(self.os.dpi_factor);
                        let size = self.os.display_size / dpi_factor;
                        window.window_geom = WindowGeom {
                            dpi_factor,
                            can_fullscreen: false,
                            xr_is_presenting: false,
                            is_fullscreen: true,
                            is_topmost: true,
                            position: dvec2(0.0, 0.0),
                            inner_size: size,
                            outer_size: size,
                        };
                        let new_geom = window.window_geom.clone();
                        self.call_event_handler(&Event::WindowGeomChange(WindowGeomChangeEvent {
                            window_id,
                            new_geom,
                            old_geom
                        }));
                        if let Some(main_pass_id) = self.windows[window_id].main_pass_id {
                            self.redraw_pass_and_child_passes(main_pass_id);
                        }
                        self.redraw_all();
                        self.os.first_after_resize = true;
                        self.call_event_handler(&Event::ClearAtlasses);
                    }
                    FromJavaMessage::Touch(mut touches) => {
                        let time = self.os.time_now();
                        let window = &mut self.windows[CxWindowPool::id_zero()];
                        let dpi_factor = window.dpi_override.unwrap_or(self.os.dpi_factor);
                        for touch in &mut touches {
                            // When the software keyboard shifted the UI in the vertical axis,
                            //we need to make the math here to keep touch events positions synchronized.
                            //if self.os.keyboard_visible {touch.abs.y += self.os.keyboard_panning_offset as f64};
                            //crate::log!("{} {:?} {} {}", time, touch.state, touch.uid, touch.abs);
                            touch.abs /= dpi_factor;
                        }
                        self.fingers.process_touch_update_start(time, &touches);
                        let e = Event::TouchUpdate(
                            TouchUpdateEvent {
                                time,
                                window_id: CxWindowPool::id_zero(),
                                touches,
                                modifiers: Default::default()
                            }
                        );
                        self.call_event_handler(&e);
                        let e = if let Event::TouchUpdate(e) = e {e}else {panic!()};
                        self.fingers.process_touch_update_end(&e.touches);
                    }
                    FromJavaMessage::Character {character} => {
                        if let Some(character) = char::from_u32(character) {
                            let e = Event::TextInput(
                                TextInputEvent {
                                    input: character.to_string(),
                                    replace_last: false,
                                    was_paste: false,
                                }
                            );
                            self.call_event_handler(&e);
                        }
                    }
                    FromJavaMessage::KeyDown {keycode, meta_state} => {
                        let e: Event;
                        let makepad_keycode = android_to_makepad_key_code(keycode);
                        if !makepad_keycode.is_unknown() {
                            let control = meta_state & ANDROID_META_CTRL_MASK != 0;
                            let alt = meta_state & ANDROID_META_ALT_MASK != 0;
                            let shift = meta_state & ANDROID_META_SHIFT_MASK != 0;
                            let is_shortcut = control || alt;
                            if is_shortcut {
                                if makepad_keycode == KeyCode::KeyC {
                                    let response = Rc::new(RefCell::new(None));
                                    e = Event::TextCopy(TextClipboardEvent {
                                        response: response.clone()
                                    });
                                    self.call_event_handler(&e);
                                    // let response = response.borrow();
                                    // if let Some(response) = response.as_ref(){
                                    //     to_java.copy_to_clipboard(response);
                                    // }
                                } else if makepad_keycode == KeyCode::KeyX {
                                    let response = Rc::new(RefCell::new(None));
                                    let e = Event::TextCut(TextClipboardEvent {
                                        response: response.clone()
                                    });
                                    self.call_event_handler(&e);
                                    // let response = response.borrow();
                                    // if let Some(response) = response.as_ref(){
                                    //     to_java.copy_to_clipboard(response);
                                    // }
                                } else if makepad_keycode == KeyCode::KeyV {
                                    //to_java.paste_from_clipboard();
                                }
                            } else {
                                e = Event::KeyDown(
                                    KeyEvent {
                                        key_code: makepad_keycode,
                                        is_repeat: false,
                                        modifiers: KeyModifiers {shift, control, alt, ..Default::default()},
                                        time: self.os.time_now()
                                    }
                                );
                                self.call_event_handler(&e);
                            }
                        }
                    }
                    FromJavaMessage::KeyUp {keycode, meta_state} => {
                        let makepad_keycode = android_to_makepad_key_code(keycode);
                        let control = meta_state & ANDROID_META_CTRL_MASK != 0;
                        let alt = meta_state & ANDROID_META_ALT_MASK != 0;
                        let shift = meta_state & ANDROID_META_SHIFT_MASK != 0;
                        
                        let e = Event::KeyUp(
                            KeyEvent {
                                key_code: makepad_keycode,
                                is_repeat: false,
                                modifiers: KeyModifiers {shift, control, alt, ..Default::default()},
                                time: self.os.time_now()
                            }
                        );
                        self.call_event_handler(&e);
                    }
                    FromJavaMessage::ResizeTextIME {keyboard_height, is_open} => {
                        let keyboard_height = (keyboard_height as f64) / self.os.dpi_factor;
                        if !is_open {
                            self.os.keyboard_closed = keyboard_height;
                        }
                        if is_open {
                            self.call_event_handler(&Event::VirtualKeyboard(VirtualKeyboardEvent::DidShow {
                                height: keyboard_height - self.os.keyboard_closed,
                                time: self.os.time_now()
                            }))
                        }
                        else {
                            self.text_ime_was_dismissed();
                            self.call_event_handler(&Event::VirtualKeyboard(VirtualKeyboardEvent::DidHide {
                                time: self.os.time_now()
                            }))
                        }
                    }
                    FromJavaMessage::HttpResponse {request_id, metadata_id, status_code, headers, body} => {
                        let mut e = Event::NetworkResponses(vec![
                            NetworkResponseEvent {
                                request_id: LiveId(request_id),
                                response: NetworkResponse::HttpResponse(HttpResponse::new(
                                    LiveId(metadata_id),
                                    status_code,
                                    headers,
                                    Some(body)
                                ))
                            }
                        ]);
                        if self.studio_http_connection(&mut e) {
                            self.call_event_handler(&e);
                        }
                    }
                    FromJavaMessage::HttpRequestError {request_id, error, ..} => {
                        let mut e = Event::NetworkResponses(vec![
                            NetworkResponseEvent {
                                request_id: LiveId(request_id),
                                response: NetworkResponse::HttpRequestError(error)
                            }
                        ]);
                        if self.studio_http_connection(&mut e) {
                            self.call_event_handler(&e);
                        }
                    }
                    FromJavaMessage::MidiDeviceOpened {name, midi_device} => {
                        self.os.media.android_midi().lock().unwrap().midi_device_opened(name, midi_device);
                    }
                    FromJavaMessage::VideoDecodingInitialized {video_id, frame_rate, video_width, video_height, color_format, duration} => {
                        let e = Event::VideoDecodingInitialized(
                            VideoDecodingInitializedEvent {
                                video_id: LiveId(video_id),
                                frame_rate,
                                video_width,
                                video_height,
                                color_format: VideoColorFormat::from_str(&color_format),
                                duration,
                            }
                        );
                        self.call_event_handler(&e);
                    },
                    FromJavaMessage::VideoStream {video_id, frames_group} => {
                        if let Some(callback_mutex) = self.os.decoding.video_decoding_input_cb.get(&LiveId(video_id)) {
                            if let Ok(mut lock) = callback_mutex.lock() {
                                if let Some(ref mut callback) = *lock {
                                    (*callback)(frames_group);
                                }
                            }
                        }
                    },
                    FromJavaMessage::VideoChunkDecoded {video_id} => {
                        let e = Event::VideoChunkDecoded(LiveId(video_id));
                        self.call_event_handler(&e);
                    },
                    FromJavaMessage::VideoDecodingError {video_id, error} => {
                        let e = Event::VideoDecodingError(
                            VideoDecodingErrorEvent {
                                video_id: LiveId(video_id),
                                error,
                            }
                        );
                        self.call_event_handler(&e);
                    },
                    FromJavaMessage::Pause => {
                        self.call_event_handler(&Event::Pause);
                    }
                    FromJavaMessage::Stop => {
                    }
                    FromJavaMessage::Resume => {
                        if self.os.fullscreen {
                            unsafe {
                                let env = attach_jni_env();
                                android_jni::to_java_set_full_screen(env, true);
                            }
                        }
                        self.redraw_all();
                        self.reinitialise_media();
                        self.call_event_handler(&Event::Resume);
                    }
                    FromJavaMessage::Destroy => {
                        self.call_event_handler(&Event::Destruct);
                        self.os.quit = true;
                    }
                    FromJavaMessage::Init(_) => {
                        panic!()
                    }
                }
            }
            
            if Signal::check_and_clear_ui_signal() {
                self.handle_media_signals();
                self.call_event_handler(&Event::Signal);
            }
            
            if self.handle_live_edit() {
                self.call_event_handler(&Event::LiveEdit);
                self.redraw_all();
            }
            self.handle_platform_ops();
            
            if self.any_passes_dirty() || self.need_redrawing() || self.new_next_frames.len() != 0 {
                if self.new_next_frames.len() != 0 {
                    self.call_next_frame_event(self.os.time_now());
                }
                if self.need_redrawing() {
                    self.call_draw_event();
                    self.opengl_compile_shaders();
                }
                
                if self.os.first_after_resize {
                    self.os.first_after_resize = false;
                    self.redraw_all();
                }
                
                self.handle_repaint();
            }
            else {
                std::thread::sleep(Duration::from_millis(8));
            }
        }
    }
    
    pub fn android_entry<F>(activity: *const std::ffi::c_void, startup: F) where F: FnOnce() -> Box<Cx> + Send + 'static {
        let (from_java_tx, from_java_rx) = mpsc::channel();
        
        unsafe {android_jni::jni_init_globals(activity, from_java_tx)};
        
        // lets start a thread
        std::thread::spawn(move || {
            unsafe {attach_jni_env()};
            let mut cx = startup();
            cx.android_load_dependencies();
            let mut libegl = LibEgl::try_load().expect("Cant load LibEGL");
            
            let window = loop {
                match from_java_rx.try_recv() {
                    Ok(FromJavaMessage::Init(params)) => {
                        cx.os.dpi_factor = params.density;
                        cx.os_type = OsType::Android(params);
                    }
                    Ok(FromJavaMessage::SurfaceChanged {
                        window,
                        width,
                        height,
                    }) => {
                        cx.os.display_size = dvec2(width as f64, height as f64);
                        break window;
                    }
                    _ => {}
                }
            };
            let (egl_context, egl_config, egl_display) = unsafe {egl_sys::create_egl_context(
                &mut libegl,
                std::ptr::null_mut(),/* EGL_DEFAULT_DISPLAY */
                false,
            ).expect("Cant create EGL context")};
            unsafe {gl_sys::load_with( | s | {
                let s = CString::new(s).unwrap();
                libegl.eglGetProcAddress.unwrap()(s.as_ptr())
            })};
            
            let surface = unsafe {(libegl.eglCreateWindowSurface.unwrap())(
                egl_display,
                egl_config,
                window as _,
                std::ptr::null_mut(),
            )};
            
            if unsafe {(libegl.eglMakeCurrent.unwrap())(egl_display, surface, surface, egl_context)} == 0 {
                panic!();
            }
            cx.os.display = Some(CxAndroidDisplay {
                libegl,
                egl_display,
                egl_config,
                egl_context,
                surface,
                window
            });
            cx.main_loop(from_java_rx);
            
            let display = cx.os.display.take().unwrap();
            
            unsafe {
                (display.libegl.eglMakeCurrent.unwrap())(
                    display.egl_display,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
                (display.libegl.eglDestroySurface.unwrap())(display.egl_display, display.surface);
                (display.libegl.eglDestroyContext.unwrap())(display.egl_display, display.egl_context);
                (display.libegl.eglTerminate.unwrap())(display.egl_display);
            }
        });
    }
    
    pub fn studio_http_connection(&mut self, event: &mut Event) -> bool {
        if let Event::NetworkResponses(res) = event {
            res.retain( | res | {
                if res.request_id == live_id!(live_reload) {
                    // alright lets see if we need to live reload from the body
                    if let NetworkResponse::HttpResponse(res) = &res.response {
                        // lets check our response
                        if let Some(body) = res.get_string_body() {
                            if body.len()>0 {
                                let mut parts = body.split("$$$makepad_live_change$$$");
                                if let Some(file_name) = parts.next() {
                                    let content = parts.next().unwrap().to_string();
                                    let _ = self.live_file_change_sender.send(vec![LiveFileChange{
                                        file_name:file_name.to_string(),
                                        content
                                    }]);
                                }
                            }
                        }
                        Self::poll_studio_http();
                    }
                    false
                }
                else {
                    true
                }
            });
            if res.len()>0 {
                return true
            }
        }
        false
    }
    
    fn poll_studio_http() {
        let studio_http: Option<&'static str> = std::option_env!("MAKEPAD_STUDIO_HTTP");
        if studio_http.is_none() {
            return
        }
        let url = format!("http://{}/$live_file_change", studio_http.unwrap());
        let request = HttpRequest::new(url, HttpMethod::GET);
        unsafe {android_jni::to_java_http_request(live_id!(live_reload), request);}
    }
    
    pub fn start_network_live_file_watcher(&mut self) {
        Self::poll_studio_http();
        /*
        log!("WATCHING NETWORK FOR FILE WATCHER");
        let studio_uid: Option<&'static str> = std::option_env!("MAKEPAD_STUDIO_UID");
        if studio_uid.is_none(){
            return
        }
        let studio_uid:u64 = studio_uid.unwrap().parse().unwrap_or(0);
        std::thread::spawn(move || {
            let discovery = UdpSocket::bind("0.0.0.0:41533").unwrap();
            discovery.set_read_timeout(Some(Duration::new(0, 1))).unwrap();
            discovery.set_broadcast(true).unwrap();
            
            let mut other_uid = [0u8; 8];
            let mut host_addr = None;
            // nonblockingly (timeout=1ns) check our discovery socket for peers
            'outer: loop{
                while let Ok((_, mut addr)) = discovery.recv_from(&mut other_uid) {
                    let recv_uid = u64::from_be_bytes(other_uid);
                    log!("GOT ADDR {} {}",studio_uid, recv_uid);
                    if studio_uid == recv_uid {
                        // we found our host. lets connect to it
                        host_addr = Some(addr);
                        break 'outer;
                    }
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let host_addr = host_addr.unwrap();
            // ok we can connect
            log!("WE CAN CONNECT {:?}", host_addr);
        });*/
    }
    
    /*    
    pub fn from_java_on_paste_from_clipboard(&mut self, content: Option<String>, to_java: AndroidToJava) {
        if let Some(text) = content {
            let e = Event::TextInput(
                TextInputEvent {
                    input: text,
                    replace_last: false,
                    was_paste: true,
                }
            );
            self.call_event_handler(&e);
            self.after_every_event(&to_java);
        }
    }
    
    pub fn from_java_on_cut_to_clipboard(&mut self, to_java: AndroidToJava) {
        let e = Event::TextCut(
            TextClipboardEvent {
                response: Rc::new(RefCell::new(None))
            }
        );
        self.call_event_handler(&e);
        self.after_every_event(&to_java);
    }
   */
    
    
    pub fn android_load_dependencies(&mut self) {
        for (path, dep) in &mut self.dependencies {
            if let Some(data) = unsafe {to_java_load_asset(path)} {
                dep.data = Some(Ok(Rc::new(data)))
            }
            else {
                let message = format!("cannot load dependency {}", path);
                crate::makepad_error_log::error!("Android asset failed: {}", message);
                dep.data = Some(Err(message));
            }
        }
    }
    
    pub fn draw_pass_to_fullscreen(
        &mut self,
        pass_id: PassId,
    ) {
        let draw_list_id = self.passes[pass_id].main_draw_list_id.unwrap();
        
        self.setup_render_pass(pass_id);
        
        // keep repainting in a loop
        self.passes[pass_id].paint_dirty = false;
        //let panning_offset = if self.os.keyboard_visible {self.os.keyboard_panning_offset} else {0};
        
        unsafe {
            gl_sys::Viewport(0, 0, self.os.display_size.x as i32, self.os.display_size.y as i32);
        }
        
        let clear_color = if self.passes[pass_id].color_textures.len() == 0 {
            self.passes[pass_id].clear_color
        }
        else {
            match self.passes[pass_id].color_textures[0].clear_color {
                PassClearColor::InitWith(color) => color,
                PassClearColor::ClearWith(color) => color
            }
        };
        let clear_depth = match self.passes[pass_id].clear_depth {
            PassClearDepth::InitWith(depth) => depth,
            PassClearDepth::ClearWith(depth) => depth
        };
        
        if !self.passes[pass_id].dont_clear {
            unsafe {
                //gl_sys::BindFramebuffer(gl_sys::FRAMEBUFFER, 0);
                gl_sys::ClearDepthf(clear_depth as f32);
                gl_sys::ClearColor(clear_color.x, clear_color.y, clear_color.z, clear_color.w);
                gl_sys::Clear(gl_sys::COLOR_BUFFER_BIT | gl_sys::DEPTH_BUFFER_BIT);
            }
        }
        Self::set_default_depth_and_blend_mode();
        
        let mut zbias = 0.0;
        let zbias_step = self.passes[pass_id].zbias_step;
        
        self.render_view(
            pass_id,
            draw_list_id,
            &mut zbias,
            zbias_step,
        );
        
        //to_java.swap_buffers();
        //unsafe {
        //direct_app.drm.swap_buffers_and_wait(&direct_app.egl);
        //}
    }
    
    pub (crate) fn handle_repaint(&mut self) {
        //opengl_cx.make_current();
        let mut passes_todo = Vec::new();
        self.compute_pass_repaint_order(&mut passes_todo);
        self.repaint_id += 1;
        for pass_id in &passes_todo {
            self.passes[*pass_id].set_time(self.os.time_now() as f32);
            match self.passes[*pass_id].parent.clone() {
                CxPassParent::Window(_) => {
                    //let window = &self.windows[window_id];
                    self.draw_pass_to_fullscreen(*pass_id);
                    unsafe {
                        if let Some(display) = &mut self.os.display {
                            (display.libegl.eglSwapBuffers.unwrap())(display.egl_display, display.surface);
                            
                        }
                    }
                }
                CxPassParent::Pass(_) => {
                    //let dpi_factor = self.get_delegated_dpi_factor(parent_pass_id);
                    self.draw_pass_to_magic_texture(*pass_id);
                },
                CxPassParent::None => {
                    self.draw_pass_to_magic_texture(*pass_id);
                }
            }
        }
        
        
    }
    
    fn handle_platform_ops(&mut self) -> EventFlow {
        while let Some(op) = self.platform_ops.pop() {
            match op {
                CxOsOp::CreateWindow(window_id) => {
                    let window = &mut self.windows[window_id];
                    let dpi_factor = window.dpi_override.unwrap_or(self.os.dpi_factor);
                    let size = self.os.display_size / dpi_factor;
                    window.window_geom = WindowGeom {
                        dpi_factor,
                        can_fullscreen: false,
                        xr_is_presenting: false,
                        is_fullscreen: true,
                        is_topmost: true,
                        position: dvec2(0.0, 0.0),
                        inner_size: size,
                        outer_size: size,
                    };
                    window.is_created = true;
                },
                CxOsOp::SetCursor(_cursor) => {
                    //xlib_app.set_mouse_cursor(cursor);
                },
                CxOsOp::StartTimer {timer_id, interval, repeats} => {
                    self.os.timers.insert(timer_id, Timer::new(interval, repeats));
                },
                CxOsOp::StopTimer(timer_id) => {
                    self.os.timers.remove(&timer_id);
                },
                CxOsOp::ShowTextIME(_area, _pos) => {
                    //self.os.keyboard_trigger_position = area.get_clipped_rect(self).pos;
                    unsafe {android_jni::to_java_show_keyboard(true);}
                },
                CxOsOp::HideTextIME => {
                    //self.os.keyboard_visible = false;
                    unsafe {android_jni::to_java_show_keyboard(false);}
                },
                CxOsOp::ShowClipboardActions(_selected) => {
                    //to_java.show_clipboard_actions(selected.as_str());
                },
                CxOsOp::HttpRequest {request_id, request} => {
                    unsafe {android_jni::to_java_http_request(request_id, request);}
                },
                CxOsOp::InitializeVideoDecoding(video_id, video, chunk_size) => {
                    unsafe {
                        let env = attach_jni_env();
                        android_jni::to_java_initialize_video_decoding(env, video_id, video, chunk_size);
                    }
                },
                CxOsOp::DecodeNextVideoChunk(video_id, max_frames_to_decode) => {
                    unsafe {
                        let env = attach_jni_env();
                        android_jni::to_java_decode_next_video_chunk(env, video_id, max_frames_to_decode);
                    }
                },
                CxOsOp::FetchNextVideoFrames(video_id, number_frames) => {
                    unsafe {
                        let env = attach_jni_env();
                        android_jni::to_java_fetch_next_video_frames(env, video_id, number_frames);
                    }
                },
                CxOsOp::CleanupVideoDecoding(video_id) => {
                    unsafe {
                        let env = attach_jni_env();
                        android_jni::to_java_cleanup_video_decoding(env, video_id);
                    }
                }
                _ => ()
            }
        }
        EventFlow::Poll
    }
    
    fn handle_timers(&mut self) {
        let mut to_be_dispatched = Vec::with_capacity(self.os.timers.len());
        let mut to_be_removed = Vec::with_capacity(self.os.timers.len());
        let now = Instant::now();
        let time = self.os.time_now();
        for (id, timer) in self.os.timers.iter_mut() {
            let elapsed_time = now - timer.start_time;
            let next_due_time = Duration::from_nanos(timer.interval.as_nanos() as u64 * (timer.step + 1));
            
            if elapsed_time > next_due_time {
                
                to_be_dispatched.push(Event::Timer(TimerEvent {timer_id: *id, time:Some(time)}));
                if timer.repeats {
                    timer.step += 1;
                } else {
                    to_be_removed.push(*id);
                }
            }
        }
        
        for id in to_be_removed {
            self.os.timers.remove(&id);
        }
        for event in to_be_dispatched {
            self.call_event_handler(&event);
        }
        self.os.last_time = now;
    }
}

impl CxOsApi for Cx {
    fn init_cx_os(&mut self) {
        self.live_registry.borrow_mut().package_root = Some("makepad".to_string());
        self.live_expand();
        self.live_scan_dependencies();
    }
    
    fn spawn_thread<F>(&mut self, f: F) where F: FnOnce() + Send + 'static {
        std::thread::spawn(f);
    }
}

impl Default for CxOs {
    fn default() -> Self {
        Self {
            last_time: Instant::now(),
            first_after_resize: true,
            display_size: dvec2(100., 100.),
            dpi_factor: 1.5,
            time_start: Instant::now(),
            keyboard_closed: 0.0,
            //keyboard_visible: false,
            //keyboard_trigger_position: DVec2::default(),
            //keyboard_panning_offset: 0,
            media: CxAndroidMedia::default(),
            decoding: CxAndroidDecoding::default(),
            display: None,
            quit: false,
            fullscreen: false,
            timers: HashMap::new()
        }
    }
}

pub struct CxAndroidDisplay {
    libegl: LibEgl,
    egl_display: egl_sys::EGLDisplay,
    egl_config: egl_sys::EGLConfig,
    egl_context: egl_sys::EGLContext,
    surface: egl_sys::EGLSurface,
    window: *mut ndk_sys::ANativeWindow,
    //event_handler: Box<dyn EventHandler>,
}


pub struct CxOs {
    pub last_time: Instant,
    pub first_after_resize: bool,
    pub display_size: DVec2,
    pub dpi_factor: f64,
    pub time_start: Instant,
    pub keyboard_closed: f64,
    //pub keyboard_visible: bool,
    //pub keyboard_trigger_position: DVec2,
    //pub keyboard_panning_offset: i32,
    
    pub quit: bool,
    pub fullscreen: bool,
    pub (crate) display: Option<CxAndroidDisplay>,
    pub (crate) media: CxAndroidMedia,
    pub (crate) decoding: CxAndroidDecoding,
    pub (crate) timers: HashMap<u64, Timer>,
}

impl CxAndroidDisplay {
    unsafe fn destroy_surface(&mut self) {
        (self.libegl.eglMakeCurrent.unwrap())(
            self.egl_display,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        (self.libegl.eglDestroySurface.unwrap())(self.egl_display, self.surface);
        self.surface = std::ptr::null_mut();
    }
    
    unsafe fn update_surface(&mut self, window: *mut ndk_sys::ANativeWindow) {
        if !self.window.is_null() {
            ndk_sys::ANativeWindow_release(self.window);
        }
        self.window = window;
        if self.surface.is_null() == false {
            self.destroy_surface();
        }
        
        self.surface = (self.libegl.eglCreateWindowSurface.unwrap())(
            self.egl_display,
            self.egl_config,
            window as _,
            std::ptr::null_mut(),
        );
        
        assert!(!self.surface.is_null());
        
        let res = (self.libegl.eglMakeCurrent.unwrap())(
            self.egl_display,
            self.surface,
            self.surface,
            self.egl_context,
        );
        
        assert!(res != 0);
    }
}

impl CxOs {
    pub fn time_now(&self) -> f64 {
        let time_now = Instant::now(); //unsafe {mach_absolute_time()};
        (time_now.duration_since(self.time_start)).as_micros() as f64 / 1_000_000.0
    }
}

pub struct Timer {
    pub start_time: Instant,
    pub interval: Duration,
    pub repeats: bool,
    pub step: u64,
}

impl Timer {
    pub fn new(interval_ms: f64, repeats: bool) -> Timer {
        let interval_ns = (interval_ms * 1e6) as u64;
        Timer {
            start_time: Instant::now(),
            interval: Duration::from_nanos(interval_ns),
            repeats,
            step: 0,
        }
    }
}