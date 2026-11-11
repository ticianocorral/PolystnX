//! Gabinete: scenes (extraído de `cabinet/mod.rs`, sem mudança de comportamento).

use super::*;

impl Cabinet {
    /// Present of every frame, through the trace watchdog (`PSX_XPERIENCE_TRACE=
    /// 1`): present is where a GPU/compositor/display stall shows up — telling
    /// it apart from a slow app loop is the whole point of the trace.
    /// O `present` das cenas com tubo: o lado TV (vidro da fase, OSD de
    /// volume e controles do queixo) entra por cima de tudo, uma vez só.
    pub(super) fn present_tube(&mut self) {
        self.draw_tv_overlay();
        self.present_and_time();
    }

    pub(super) fn present_and_time(&mut self) {
        let _span = crate::TraceSpan::new("present");
        // DEBUG-CI: dump do composto em frames marcados (diagnóstico headless).
        if let Some(path) = debug_composite_path() {
            use std::sync::atomic::{AtomicU32, Ordering};
            static N: AtomicU32 = AtomicU32::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            if n == 60 || n == 240 || n == 600 || n == 1200 {
                let named = format!("{path}.{n}.bmp");
                let _ = self
                    .canvas
                    .read_pixels(None::<Rect>)
                    .and_then(|s| s.save_bmp(std::path::Path::new(&named)));
                log::info!("debug composite: frame {n} -> {named}");
            }
        }
        self.canvas.present();
    }

    /// Algo se mexe sozinho no gabinete (disco girando ou parando, aviso com
    /// prazo no OSD, canal na tela): quem pace pelo ritmo ocioso segura os
    /// 60 Hz. Conservador — na dúvida, `true`.
    pub fn animating(&self) -> bool {
        self.spin_speed > 0.0
            || !self.osd_queue.is_empty()
            || self.ch3_until.is_some()
            || self.tv_transitioning()
            || self.volume_osd_until.is_some()
            || self.tv_menu.is_some()
    }

    /// Draw the modal dialog DENTRO da TV, sobre a cena completa (plan
    /// revision: "igual ao SNES — as coisas abrem na tv sem a interface
    /// mudar"): o gabinete, o painel e a TV são REDESENHADOS (estática com
    /// o console desligado; o último quadro do jogo ligado) e o dialog é
    /// composto dentro da área do tubo. Nada some.
    /// Draw the modal dialog to the window, replacing the whole window like
    /// the pause book does (plan revision) — IGUAL AO SNES.
    /// Draw the modal dialog DENTRO DA TV, sobre a cena completa (plan
    /// revision: "as coisas abrem na tv sem a interface mudar"): a cena é
    /// REDESENHADA inteira (gabinete, painel, TV com estática ou o último
    /// quadro do jogo) e só então o dialog é composto sobre a área do
    /// tubo. Nada é limpo nem substituído.
    pub fn present_modal(&mut self, backdrop: ModalBackdrop) {
        match backdrop {
            ModalBackdrop::Static(level) => self.render_static_scene(level),
            ModalBackdrop::Frame { aspect_ratio } => {
                // Sem quadro retido (modal aberto antes do primeiro quadro
                // apresentado) — cai na estática.
                if self.src.is_some() {
                    self.render_frame_scene_last(aspect_ratio);
                } else {
                    self.render_static_scene(0.0);
                }
            }
        }
        // O dialog, centrado na ÁREA DO TUBO — desenhado DENTRO do mesmo
        // viewport da cena (canvas-relative): em janelas mais largas que
        // 16:9 o canvas fica centrado com offset, e desenhar fora do
        // viewport deslocava o véu/cartão (e os rects de clique) da TV.
        self.canvas.set_viewport(Some(self.canvas_rect));
        let screen = self.screen;
        self.modal_buttons = draw_modal(
            &mut self.canvas,
            &mut self.font,
            self.modal.as_ref(),
            screen,
        );
        self.canvas.set_viewport(None);
        // DEBUG-CI: dump do composto (o que a Metal desenhou de fato).
        if let Some(path) = debug_composite_path() {
            static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n == 5 || n == 30 || n == 300 {
                let named = format!("{path}.{n}.bmp");
                let _ = self
                    .canvas
                    .read_pixels(None::<Rect>)
                    .and_then(|s| s.save_bmp(std::path::Path::new(&named)));
                log::info!("debug composite: {} -> {named}", n);
            }
        }
        self.present_and_time();
    }

    /// Like [`Cabinet::present_modal`] but composited into an offscreen
    /// target and saved as a BMP (headless preview).
    pub fn capture_modal_bmp(&mut self, path: &std::path::Path) -> Result<(), PlatformError> {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, real_w, real_h)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let modal = self.modal.as_ref();
        let font = &mut self.font;
        let screen = screen_area(
            rect.width()
                .saturating_sub(panel_rect(rect.width(), rect.height()).width()),
            rect.height(),
        );
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(rect));
            let _ = draw_modal(c, font, modal, screen);
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Draw the pause book to the window: two pages, not warped by the tube,
    /// replacing the whole window rather than sharing it with the cabinet
    /// (plan §3.2/§3.4), plus its own clickable "Continuar" button — mouse/
    /// gamepad only, no keyboard (plan revision: the frame-step button that
    /// used to sit next to it is gone, a dev-only leftover nobody used
    /// through the actual UI).
    pub fn present_pause(&mut self) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = rect;
        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(rect));
        self.pause_buttons = draw_pause_book(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.pause.as_ref(),
            rect.width(),
            rect.height(),
        );
        // O nameplate (app + core) nas outras telas do chin, também aqui —
        // (plan revision: "a versão do app e a do pcsx em todas as telas").
        self.update_arrows = draw_brand(
            &mut self.canvas,
            &mut self.font,
            rect,
            real_h,
            self.nameplate.as_str(),
            self.nameplate_updates,
        );
        self.canvas.set_viewport(None);
        self.present_and_time();
    }

    /// Like [`Cabinet::present_pause`] but composited into an offscreen
    /// target and saved as a BMP (headless preview).
    pub fn capture_pause_bmp(&mut self, path: &std::path::Path) -> Result<(), PlatformError> {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, real_w, real_h)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let pause = self.pause.as_ref();
        let font = &mut self.font;
        let images = &self.images;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(rect));
            let _ = draw_pause_book(c, font, images, pause, rect.width(), rect.height());
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Whether image `id` is already decoded and cached — e.g. to decide
    /// whether any cover art exists at all before choosing how to lay out
    /// the shelf (grid vs. list, plan §3.1). Same lookup `Screen::has_image`
    /// does from inside a `frame_2d` closure, exposed here for callers that
    /// need the answer *before* they can build that closure.
    pub fn has_image(&self, id: u64) -> bool {
        self.images.contains_key(&id)
    }

    /// O tamanho do buffer 2D que o `paint_shelf` usa (canvas − painel −
    /// bezel) — o MESMO espaço que a tela de setup desenha e hit-testa.
    /// Diferente do [`Self::screen_size`] (canvas cheio): em ultrawide o
    /// painel estreita o buffer e os rects divergem se um lado usar um.
    pub fn shelf_buffer_size(&self) -> (u32, u32) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        let cab_w = rect
            .width()
            .saturating_sub(panel_rect(rect.width(), rect.height()).width());
        let s = screen_area(cab_w, rect.height());
        (s.width(), s.height())
    }

    /// Size of the recessed screen area — what the selector lays itself out in.
    pub fn screen_size(&self) -> (u32, u32) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        let s = screen_area(rect.width(), rect.height());
        (s.width(), s.height())
    }

    /// Like [`Cabinet::screen_size`], but for the shelf specifically (plan
    /// revision): the tube's own area minus the flat side panel, since the
    /// shelf's grid and the game's video now share the exact same split
    /// (`panel_rect` carved out of `cabinet_canvas_rect` first) instead of
    /// the panel living inside the warped buffer.
    pub fn shelf_screen_size(&self) -> (u32, u32) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        let (out_w, out_h) = (rect.width(), rect.height());
        let cab_w = out_w.saturating_sub(panel_rect(out_w, out_h).width());
        let s = screen_area(cab_w, out_h);
        (s.width(), s.height())
    }

    // --- game path -------------------------------------------------------

    /// Draw one frame to the window. `aspect_ratio <= 0` means "use 4:3".
    pub fn present_frame(&mut self, frame: &FrameRef, aspect_ratio: f32) {
        self.render_frame_scene(frame, aspect_ratio);
        self.present_tube();
    }

    /// A cena inteira do gameplay (tubo com o quadro, bezel, marca, OSD,
    /// painel, fechar/minimizar) desenhada no backbuffer — SEM apresentar.
    /// O `present_frame` chama isto e apresenta; o `present_modal` reusa
    /// para compor o dialog POR CIMA da última cena (a interface não muda).
    pub(super) fn render_frame_scene(&mut self, frame: &FrameRef, aspect_ratio: f32) {
        self.ensure_src(frame.width, frame.height, frame.format);
        self.upload(frame);
        self.render_frame_scene_last(aspect_ratio);
    }

    /// Um passo da RAMPA do motor do leitor — o disco gira a 420 °/s com o
    /// console ligado, disco assentado e tampa fechada; sem qualquer um
    /// desses, o alvo é 0 e o giro COASTA (aceleração na arrancada, freio
    /// na parada, °/s²: ~1.5 s até o giro cheio, ~2 s até parar), SEMPRE
    /// parando no ângulo em que está. Por relógio: FMV pesada atrasa o
    /// QUADRO, não a velocidade do disco. Chamado por toda cena que
    /// desenha o painel — o desligar troca a cena (`render_frame_scene` →
    /// `render_static_scene`), e sem o tick lá o disco congelava seco no
    /// clique do Power em vez de frear com inércia.
    pub(super) fn tick_spin(&mut self, spinning: bool) {
        let now = Instant::now();
        let dt = self
            .spin_last
            .replace(now)
            .map_or(0.0, |t| (now - t).as_secs_f32())
            .min(0.1);
        let target = if spinning { 420.0 } else { 0.0 };
        if self.spin_speed != target {
            let rate = if target > self.spin_speed {
                280.0
            } else {
                210.0
            };
            self.spin_speed = if target > self.spin_speed {
                (self.spin_speed + rate * dt).min(target)
            } else {
                (self.spin_speed - rate * dt).max(target)
            };
        }
        if self.spin_speed > 0.0 {
            self.spin = (self.spin + self.spin_speed * dt) % 360.0;
        }
    }

    /// O quadro JÁ carregado (`self.src`, retido pelo `present_frame`),
    /// redesenhado na cena completa — o backdrop do modal em jogo.
    pub(super) fn render_frame_scene_last(&mut self, aspect_ratio: f32) {
        let spinning = self
            .panel
            .as_ref()
            .is_some_and(|p| p.has_cartridge && p.powered && !p.lid_open);
        self.tick_spin(spinning);
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = canvas_rect;
        let (out_w, out_h) = (canvas_rect.width(), canvas_rect.height());
        let panel = panel_rect(out_w, out_h);
        let cab_w = out_w.saturating_sub(panel.width());
        let screen = screen_area(cab_w, out_h);
        self.screen = screen;
        let dst = fit_aspect_in(screen, resolve_aspect(aspect_ratio));
        self.ensure_mesh(dst);
        self.ensure_bezel(cab_w, out_h, dst);

        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(canvas_rect));

        let src = self.src.take().unwrap();
        let mesh = self.mesh.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&mesh.verts, Some(&src.tex), &mesh.indices[..]);
        self.draw_tv_glass_before_bezel(&mesh);
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.src = Some(src);
        self.mesh = Some(mesh);
        self.bezel = Some(bezel);

        self.update_arrows = draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            out_h,
            &self.nameplate,
            self.nameplate_updates,
        );
        self.draw_osd_front();
        // The timed "CH 3" flash (plan revision): power-on shows the channel
        // banner over the picture for a few seconds, then it's gone. Expired
        // deadlines clear here so the banner truly disappears from the frame
        // instead of relying on the next flash.
        if self.ch3_until.is_some_and(|until| Instant::now() >= until) {
            self.ch3_until = None;
        }
        if self.ch3_until.is_some() {
            draw_ch3_osd(&mut self.canvas, &mut self.font, self.screen);
        }
        // The picture is back, so any queued hiss goes — cut, not faded
        // (same rule the power bursts use for their own buzz).
        if let Some(h) = &self.hiss {
            h.clear();
        }
        // Fechar/minimizar on every screen, gameplay included (plan
        // revision: "mostrar o fechar e minimizar em todas as telas").
        self.close_button = draw_close_button(&mut self.canvas, &mut self.font);
        self.minimize_button = draw_minimize_button(&mut self.canvas, &mut self.font);
        self.panel_buttons = draw_panel(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.panel.as_ref(),
            self.spin,
            panel,
            self.session,
            self.idle_core_prompt.as_deref(),
            self.dev_mode,
            self.core_status.as_deref(),
        );
        // Painel de desempenho do modo dev: NA MOLDURA, na faixa acima da
        // tela (o "queixo de cima"), centralizado sobre a TV — por último,
        // ainda dentro do viewport do gabinete.
        if self.perf_overlay {
            let line = perf_line(self.perf_fps, self.perf_cpu, self.perf_ram_mb);
            let band = (canvas_rect.top(), self.screen.top() - canvas_rect.top());
            draw_perf_overlay(&mut self.canvas, &mut self.font, self.screen, band, &line);
        }
        self.canvas.set_viewport(None);
    }

    /// Render one frame into an offscreen target and save it as a BMP. Works
    /// headless (a background window never composites on macOS), so the real
    /// output — cabinet, CRT warp and all — can be eyeballed without a window.
    pub fn capture_bmp(
        &mut self,
        frame: &FrameRef,
        aspect_ratio: f32,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        self.ensure_src(frame.width, frame.height, frame.format);
        self.upload(frame);

        let (real_w, real_h) = self
            .canvas
            .output_size()
            .unwrap_or((frame.width, frame.height));
        let canvas_rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = canvas_rect;
        let (out_w, out_h) = (canvas_rect.width(), canvas_rect.height());
        let panel = panel_rect(out_w, out_h);
        let cab_w = out_w.saturating_sub(panel.width());
        let screen = screen_area(cab_w, out_h);
        self.screen = screen;
        let dst = fit_aspect_in(screen, resolve_aspect(aspect_ratio));
        self.ensure_mesh(dst);
        self.ensure_bezel(cab_w, out_h, dst);

        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, real_w, real_h)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;

        let src = self.src.take().unwrap();
        let mesh = self.mesh.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let panel_info = self.panel.as_ref();
        let session = self.session;
        let idle_core_prompt = self.idle_core_prompt.as_deref();
        let dev_mode = self.dev_mode;
        let core_status = self.core_status.as_deref();
        let osd = self.osd_for_capture();
        let font = &mut self.font;
        let images = &self.images;
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            let _ = c.render_geometry(&mesh.verts, Some(&src.tex), &mesh.indices[..]);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            let _ = draw_brand(c, font, screen, out_h, nameplate, nameplate_updates);
            match &osd {
                ChinOsd::Notify { lines, badge } => {
                    draw_chin_osd(c, font, images, screen, out_h, nameplate, lines, *badge);
                }
                ChinOsd::Ra { hardcore } => {
                    draw_chin_ra(c, font, images, screen, out_h, nameplate, *hardcore);
                }
                ChinOsd::None => {}
            }
            draw_panel(
                c,
                font,
                images,
                panel_info,
                self.spin,
                panel,
                session,
                idle_core_prompt,
                dev_mode,
                core_status,
            );
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        self.src = Some(src);
        self.mesh = Some(mesh);
        self.bezel = Some(bezel);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    pub(super) fn upload(&mut self, frame: &FrameRef) {
        let expected_pitch = frame.width as usize * frame.format.bytes_per_pixel();
        let pitch = if frame.pitch == 0 {
            expected_pitch
        } else {
            frame.pitch
        };
        let picture = self.picture;
        let mut buf = std::mem::take(&mut self.picture_buf);
        let mut tmp = std::mem::take(&mut self.picture_tmp);
        let Some(src) = self.src.as_mut() else {
            return;
        };
        // Ajustes de imagem do menu da TV: só com algo fora do neutro e no
        // formato que o filtro entende; no padrão o quadro vai direto.
        let filtered =
            frame.format == PixelFormat::Xrgb8888 && (!picture.is_neutral() || self.composite);
        let r = if filtered {
            let (w, h) = (frame.width as usize, frame.height as usize);
            picture::apply(frame.pixels, w, h, pitch, picture, &mut buf, &mut tmp);
            if self.composite {
                composite::apply(&mut buf, w, h);
            }
            src.tex.update(None, &buf, frame.width as usize * 4)
        } else {
            src.tex.update(None, frame.pixels, pitch)
        };
        self.picture_buf = buf;
        self.picture_tmp = tmp;
        // Perda do dispositivo de vídeo não derruba o app: o quadro é pulado.
        if let Err(e) = r {
            log::warn!("upload do quadro falhou ({e}) — quadro pulado");
        }
    }

    pub(super) fn ensure_src(&mut self, w: u32, h: u32, format: PixelFormat) {
        let stale = match &self.src {
            Some(s) => s.w != w || s.h != h || s.format != format,
            None => true,
        };
        if stale {
            log::debug!(
                "ensure_src: criando {}x{} {:?} (era {:?})",
                w,
                h,
                format,
                self.src.as_ref().map(|s| (s.w, s.h, s.format))
            );
            let mut tex = self
                .canvas
                .create_texture_streaming(format.sdl(), w, h)
                .expect("create streaming texture");
            tex.set_scale_mode(SdlScaleMode::Linear);
            self.src = Some(SrcTexture { tex, w, h, format });
        }
    }

    /// O mesh do caminho 2D em cache (a tela inteira, sem warp de aspecto):
    /// `mesh_2d` quando o tamanho confere, `None` para reconstruir.
    pub(super) fn mesh_2d(&mut self) -> CrtMesh {
        let (w, h) = (self.screen.width(), self.screen.height());
        match &self.mesh_2d {
            Some(m) if m.w == w && m.h == h => {}
            _ => self.mesh_2d = Some(build_crt_mesh(self.screen, 1.0)),
        }
        self.mesh_2d.take().unwrap()
    }

    pub(super) fn ensure_mesh(&mut self, dst: Rect) {
        let (w, h) = (dst.width(), dst.height());
        if matches!(&self.mesh, Some(m) if m.w == w && m.h == h) {
            return;
        }
        self.mesh = Some(build_crt_mesh(dst, 1.0));
    }

    pub(super) fn ensure_bezel(&mut self, out_w: u32, out_h: u32, screen: Rect) {
        let key = (out_w, out_h, screen.width(), screen.height());
        if matches!(&self.bezel, Some(b) if b.key == key) {
            return;
        }
        self.bezel = Some(build_bezel_mesh(out_w, out_h, screen, key));
    }

    // --- 2D path (selector) --------------------------------------------
    //
    // The selector draws flat 2D into an offscreen buffer the size of the tube
    // opening, which is then composited through the CRT mesh just like a game
    // frame — so the shelf bulges with the same tube.

    /// Register/replace an image from tightly-packed RGBA8 (call between frames).
    /// A logo do memory card: a arte da console logo com modulação PRETA —
    /// o card desenhado no slot leva a marca em silhueta, como na foto do
    /// card real (a Sony lá é cinza-escuro em relevo).
    pub fn set_mc_logo(&mut self, w: u32, h: u32, rgba: &[u8]) {
        self.set_image(MC_LOGO_IMG, w, h, rgba);
        if let Some(img) = self.images.get_mut(&MC_LOGO_IMG) {
            img.tex.set_color_mod(0, 0, 0);
        }
    }

    /// Atualiza uma imagem "viva" — redesenhada a cada quadro por quem a
    /// registra (a tela de baixa resolução do Cheater Fish): reaproveita a
    /// textura enquanto o tamanho não muda e amplia SEM filtro, para os
    /// pixels grossos ficarem nítidos como num console de 32 bits.
    pub fn update_live_image(&mut self, id: u64, w: u32, h: u32, rgba: &[u8]) {
        if w == 0 || h == 0 || rgba.len() < (w * h * 4) as usize {
            return;
        }
        if let Some(img) = self.images.get_mut(&id) {
            if img.w == w && img.h == h {
                let _ = img.tex.update(None, rgba, (w * 4) as usize);
                return;
            }
        }
        let Ok(mut tex) = self.canvas.create_texture_static(SdlFormat::RGBA32, w, h) else {
            return;
        };
        if tex.update(None, rgba, (w * 4) as usize).is_err() {
            return;
        }
        tex.set_blend_mode(BlendMode::None);
        tex.set_scale_mode(SdlScaleMode::Nearest);
        self.images.insert(
            id,
            ImgTex {
                tex,
                w,
                h,
                vazado: false,
            },
        );
    }

    pub fn set_image(&mut self, id: u64, w: u32, h: u32, rgba: &[u8]) {
        if w == 0 || h == 0 || rgba.len() < (w * h * 4) as usize {
            return;
        }
        let vazado = rgba.as_chunks::<4>().0.iter().any(|p| p[3] < 250);
        // Arte vazada (render 3D em ângulo): a matte do render —
        // anti-aliasing CINZA em volta da caixa — vira franja cinza quando a
        // textura é reduzida com filtro linear. No upload, limpa: semi cinza
        // sai do todo. Em seguida, SANGRIA DE BORDA: o rgb dos texels
        // vazados herda a cor do conteúdo vizinho — com filtro linear, o
        // contorno mistura caixa com caixa (e não com preto/cinza), e quem
        // faz o fade é só o alpha: transparência de verdade, sem halo
        // escuro na volta nem sombra na base.
        let rgba_limpa;
        let rgba = if vazado {
            let mut limpa = rgba.to_vec();
            for p in limpa.as_chunks_mut::<4>().0 {
                let (r, g, b, a) = (p[0], p[1], p[2], p[3]);
                if a > 0 && a < 250 && r.abs_diff(g) < 24 && g.abs_diff(b) < 24 {
                    p[3] = 0;
                }
            }
            let (wu, hu) = (w as usize, h as usize);
            for _ in 0..2 {
                let fonte = limpa.clone();
                for y in 0..hu {
                    for x in 0..wu {
                        let i = (y * wu + x) * 4;
                        if fonte[i + 3] != 0 {
                            continue;
                        }
                        let vizinho = [
                            x.checked_sub(1).map(|nx| (nx, y)),
                            Some((x + 1, y)).filter(|&(nx, _)| nx < wu),
                            y.checked_sub(1).map(|ny| (x, ny)),
                            Some((x, y + 1)).filter(|&(_, ny)| ny < hu),
                        ]
                        .into_iter()
                        .flatten()
                        .find(|&(nx, ny)| fonte[(ny * wu + nx) * 4 + 3] > 0);
                        if let Some((nx, ny)) = vizinho {
                            let j = (ny * wu + nx) * 4;
                            limpa[i] = fonte[j];
                            limpa[i + 1] = fonte[j + 1];
                            limpa[i + 2] = fonte[j + 2];
                        }
                    }
                }
            }
            rgba_limpa = limpa;
            &rgba_limpa
        } else {
            rgba
        };
        let mut tex = match self.canvas.create_texture_static(SdlFormat::RGBA32, w, h) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("cabinet: texture {w}x{h}: {e}");
                return;
            }
        };
        if tex.update(None, rgba, (w * 4) as usize).is_err() {
            return;
        }
        tex.set_blend_mode(BlendMode::Blend);
        tex.set_scale_mode(SdlScaleMode::Linear);
        self.images.insert(id, ImgTex { tex, w, h, vazado });
    }

    /// A imagem `id` tem transparência de verdade (fundo vazado) — ver
    /// [`ImgTex::vazado`]. Imagem desconhecida: `false`.
    pub fn image_vazado(&self, id: u64) -> bool {
        self.images.get(&id).is_some_and(|img| img.vazado)
    }

    /// Draw a 2D frame: `draw` renders into a screen-sized buffer (coords
    /// 0..screen), which is then warped through the tube, framed and presented.
    pub fn frame_2d<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        self.paint_2d(bg, draw);
        self.composite_screen();
        self.present_tube();
    }

    /// Like [`Cabinet::frame_2d`], but for the shelf specifically (plan
    /// revision): `draw` only ever sees the narrower grid area (the panel
    /// column is reserved first), and the shelf's own info panel is drawn
    /// flat, outside the tube's warp, right after it.
    pub fn frame_shelf<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        self.paint_shelf(bg, draw);
        self.composite_shelf();
        self.present_tube();
    }

    /// Like [`Cabinet::frame_2d`] but composited into an offscreen target and
    /// saved as a BMP (headless — a background window never composites on macOS).
    pub fn capture_2d<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        self.paint_2d(bg, draw);

        let (ww, wh) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = self.canvas_rect;
        let screen = self.screen;
        let mesh = build_crt_mesh(screen, 1.0);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, ww, wh)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let st = self.screen_tex.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let font = &mut self.font;
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut close_button = self.close_button;
        let mut minimize_button = self.minimize_button;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            let _ = c.render_geometry(&mesh.verts, Some(&st.tex), &mesh.indices[..]);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            draw_brand(
                c,
                font,
                screen,
                canvas_rect.height(),
                nameplate,
                nameplate_updates,
            );
            close_button = draw_close_button(c, font);
            minimize_button = draw_minimize_button(c, font);
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        self.close_button = close_button;
        self.minimize_button = minimize_button;
        self.screen_tex = Some(st);
        self.bezel = Some(bezel);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Like [`Cabinet::capture_2d`], but for the shelf specifically (plan
    /// revision) — reserves the panel column like `frame_shelf` does, and
    /// draws the flat info panel into the same offscreen target so the BMP
    /// matches what a real window would show.
    pub fn capture_shelf<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        self.paint_shelf(bg, draw);

        let (ww, wh) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = self.canvas_rect;
        let panel = panel_rect(canvas_rect.width(), canvas_rect.height());
        let screen = self.screen;
        let mesh = build_crt_mesh(screen, 1.0);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, ww, wh)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let st = self.screen_tex.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let font = &mut self.font;
        let images = &self.images;
        let shelf_panel = self.shelf_panel.as_ref();
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut close_button = self.close_button;
        let mut minimize_button = self.minimize_button;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            let _ = c.render_geometry(&mesh.verts, Some(&st.tex), &mesh.indices[..]);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            draw_brand(
                c,
                font,
                screen,
                canvas_rect.height(),
                nameplate,
                nameplate_updates,
            );
            let _ = draw_shelf_panel(c, font, images, shelf_panel, panel);
            close_button = draw_close_button(c, font);
            minimize_button = draw_minimize_button(c, font);
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        self.close_button = close_button;
        self.minimize_button = minimize_button;
        self.screen_tex = Some(st);
        self.bezel = Some(bezel);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }
}
