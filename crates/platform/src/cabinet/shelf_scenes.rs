//! Gabinete: shelf_scenes (extraído de `cabinet/mod.rs`, sem mudança de comportamento).

use super::*;

impl Cabinet {
    /// A transição estante → console (plano §6): o último quadro da estante
    /// (guardado em `screen_tex`) dissolve sobre o azul de AV — `t` 0.0..=1.0,
    /// 0 = estante inteira, 1 = só a tela azul. Mesmo tubo, mesma TV: nada de
    /// "outra tela abrindo por cima".
    pub fn present_shelf_fade(&mut self, level: f32, t: f32) {
        let t = t.clamp(0.0, 1.0);
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = canvas_rect;
        let (ww, wh) = (canvas_rect.width(), canvas_rect.height());
        let panel = panel_rect(ww, wh);
        let cab_w = ww.saturating_sub(panel.width());
        self.screen = screen_area(cab_w, wh);
        self.ensure_bezel(cab_w, wh, self.screen);
        self.ensure_mesh(self.screen);
        self.update_noise_tex(level);

        // Idêntico ao present_static — painel, brand, OSDs e botões no lugar —
        // trocando SÓ o conteúdo do tubo: o azul de AV por baixo e o último
        // quadro da estante (screen_tex) com alpha caindo por cima. O painel
        // permanece contínuo: muda o que está na TV, não a tela inteira.
        let mesh = self.mesh.take().unwrap();
        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(canvas_rect));
        let st = self.screen_tex.as_mut();
        // Sem quadro da estante guardado (entrada direta no jogo — autoplay —
        // ou textura já larga), o tubo mostra só o azul de AV: nada para
        // dissolver, e nunca um unwrap em None.
        if let Some(st) = st {
            st.tex.set_alpha_mod(((1.0 - t) * 255.0) as u8);
            let _ = self
                .canvas
                .render_geometry(&mesh.verts, Some(&st.tex), &mesh.indices[..]);
            st.tex.set_alpha_mod(255);
        }
        let bezel = self.bezel.take().unwrap();
        self.draw_tv_glass_before_bezel(&mesh);
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.bezel = Some(bezel);
        self.mesh = Some(mesh);
        self.update_arrows = draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            wh,
            &self.nameplate,
            self.nameplate_updates,
        );
        draw_ch3_osd(&mut self.canvas, &mut self.font, self.screen);
        self.draw_chin_persistent();
        if self.static_hiss {
            self.queue_static_hiss(level);
        } else if let Some(h) = &self.hiss {
            h.clear();
        }
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
        self.close_button = draw_close_button(&mut self.canvas, &mut self.font);
        self.minimize_button = draw_minimize_button(&mut self.canvas, &mut self.font);
        self.canvas.set_viewport(None);
        self.present_tube();
    }

    /// One frame of signal-off snow through the tube, cartridge still visible
    /// in its slot if one is set. `level` 1.0 = a full blizzard, 0.0 = a dim,
    /// near-still hiss. Never a full-screen flash.
    pub fn present_static(&mut self, level: f32) {
        self.render_static_scene(level);
        self.present_tube();
    }

    /// A cena inteira da TV fora do ar (estática, bezel, marca, CH 3,
    /// painel) desenhada no backbuffer — SEM apresentar. O `present_static`
    /// apresenta; o `present_modal` reusa para compor o dialog por cima.
    pub(super) fn render_static_scene(&mut self, level: f32) {
        // Console fora do ar: a cena trocou, o motor não — o coast da
        // inércia do disco segue AQUI (a `panel.powered` já caiu, então é
        // freio até 0), parando no ângulo em que estiver (ver `tick_spin`).
        let spinning = self
            .panel
            .as_ref()
            .is_some_and(|p| p.has_cartridge && p.powered && !p.lid_open);
        self.tick_spin(spinning);
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = canvas_rect;
        let (ww, wh) = (canvas_rect.width(), canvas_rect.height());
        let panel = panel_rect(ww, wh);
        let cab_w = ww.saturating_sub(panel.width());
        self.screen = screen_area(cab_w, wh);
        self.ensure_bezel(cab_w, wh, self.screen);
        self.ensure_mesh(self.screen);
        self.update_noise_tex(level);

        // The mesh is cached like the gameplay path's — rebuilding 1,089
        // vertices every frame for a screen that only changes on
        // window-resize was pure waste.
        let mesh = self.mesh.take().unwrap();
        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(canvas_rect));
        let nt = self.noise_tex.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&mesh.verts, Some(&nt.tex), &mesh.indices[..]);
        self.noise_tex = Some(nt);
        let bezel = self.bezel.take().unwrap();
        self.draw_tv_glass_before_bezel(&mesh);
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.bezel = Some(bezel);
        self.mesh = Some(mesh);
        // O retorno PRECISA ser guardado: são os rects clicáveis das setas
        // verdes de update — sem isto o idle desenha as setas mas o clique
        // nunca acerta (hit_update_arrow lia uma lista sempre vazia).
        self.update_arrows = draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            wh,
            &self.nameplate,
            self.nameplate_updates,
        );
        // Sem sinal (plan revision): the channel banner stays up the whole
        // time the TV is showing snow — game inserted but powered off, the
        // idle screen, all of it — like a real set parked on channel 3.
        draw_ch3_osd(&mut self.canvas, &mut self.font, self.screen);
        // A conta ativa marca presença até na estática: o badge do RA no
        // queixo, do lado oposto ao nameplate.
        self.draw_chin_persistent();
        // The ambient hiss (opt-in via settings): follows the same level as
        // the visual snow, so a burst hisses loud and the resting hiss stays
        // a quiet background shhh.
        if self.static_hiss {
            self.queue_static_hiss(level);
        } else if let Some(h) = &self.hiss {
            h.clear();
        }
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
        self.close_button = draw_close_button(&mut self.canvas, &mut self.font);
        self.minimize_button = draw_minimize_button(&mut self.canvas, &mut self.font);
        self.canvas.set_viewport(None);
    }

    /// Like [`Cabinet::present_static`] but composited into an offscreen
    /// target and saved as a BMP (headless — eyeball the power-off / idle-off
    /// screen, cartridge and all, without a window).
    pub fn capture_static_bmp(
        &mut self,
        level: f32,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = canvas_rect;
        let (ww, wh) = (canvas_rect.width(), canvas_rect.height());
        let panel = panel_rect(ww, wh);
        let cab_w = ww.saturating_sub(panel.width());
        self.screen = screen_area(cab_w, wh);
        self.ensure_bezel(cab_w, wh, self.screen);
        self.update_noise_tex(level);
        let screen = self.screen;

        let mesh = build_crt_mesh(screen, 1.0);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, real_w, real_h)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let nt = self.noise_tex.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let panel_info = self.panel.as_ref();
        let session = self.session;
        let idle_core_prompt = self.idle_core_prompt.as_deref();
        let dev_mode = self.dev_mode;
        let core_status = self.core_status.as_deref();
        let ra_status = self.ra_status;
        let font = &mut self.font;
        let images = &self.images;
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut close_button = self.close_button;
        let mut minimize_button = self.minimize_button;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            let _ = c.render_geometry(&mesh.verts, Some(&nt.tex), &mesh.indices[..]);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            draw_brand(c, font, screen, wh, nameplate, nameplate_updates);
            draw_ch3_osd(c, font, screen);
            if let Some(status) = ra_status {
                draw_chin_ra(c, font, images, screen, wh, nameplate, status.hardcore);
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
        self.noise_tex = Some(nt);
        self.bezel = Some(bezel);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Drive the panel's cartridge slot animation (plan revision: "a
    /// animação deveria estar onde está o cartucho durante a gameplay, não
    /// uma transição"). `Some((t, ejecting))` animates the cartridge art in
    /// the panel's own cartridge block — `t` 0.0..=1.0 progress, `ejecting`
    /// mirrors the motion (see `draw_panel_slot`); every `present_static`/
    /// gameplay frame while this is set draws that progress. `None` is the
    /// resting state — the cartridge seated in the slot, which is also what
    /// `set_panel` starts with. A no-op before `set_panel`; `runner`'s two
    /// animation functions are the only callers.
    /// Captura o backbuffer corrente (headless diagnostic): o que quer que
    /// o último `present_*` tenha composto — modal incluído.
    pub fn capture_screen_bmp(&mut self, path: &std::path::Path) -> Result<(), PlatformError> {
        self.canvas
            .read_pixels(None::<Rect>)
            .and_then(|s| s.save_bmp(path))
            .map_err(|e| PlatformError::Sdl(e.to_string()))
    }

    pub fn set_cartridge_motion(&mut self, motion: Option<(f32, bool)>) {
        if let Some(panel) = &mut self.panel {
            panel.cartridge_motion = motion;
        }
    }
    /// A animação do memory card deslizando no slot (mesmo formato do
    /// cartridge_motion: `(t, inserindo)`, t 0..1).
    pub fn set_card_motion(&mut self, motion: Option<(f32, bool)>) {
        if let Some(panel) = &mut self.panel {
            panel.card_motion = motion;
        }
    }

    /// Like [`Cabinet::frame_2d`], but blended up from residual signal-off snow
    /// instead of cutting in cold: `static_level` is the snow still showing
    /// behind it, `shelf_alpha` (0..1) how much of the drawn frame shows on top
    /// (plan §3.3, "a estante entra por cima"). Call with `shelf_alpha` ramping
    /// 0.0 -> 1.0 over the first handful of frames after a game closes.
    pub fn frame_2d_fade_in<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        static_level: f32,
        shelf_alpha: f32,
    ) {
        self.paint_2d(bg, draw);
        self.update_noise_tex(static_level);
        let canvas_rect = self.canvas_rect;

        let mesh_static = self.mesh_2d();
        let mesh_shelf = build_crt_mesh(self.screen, shelf_alpha.clamp(0.0, 1.0));

        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(canvas_rect));

        let nt = self.noise_tex.take().unwrap();
        let _ = self.canvas.render_geometry(
            &mesh_static.verts,
            Some(&nt.tex),
            &mesh_static.indices[..],
        );
        self.noise_tex = Some(nt);

        let st = self.screen_tex.take().unwrap();
        let _ =
            self.canvas
                .render_geometry(&mesh_shelf.verts, Some(&st.tex), &mesh_shelf.indices[..]);
        self.screen_tex = Some(st);

        let bezel = self.bezel.take().unwrap();
        self.draw_tv_glass_before_bezel(&mesh_static);
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.bezel = Some(bezel);
        draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            canvas_rect.height(),
            &self.nameplate,
            self.nameplate_updates,
        );

        self.canvas.set_viewport(None);
        self.present_tube();
    }

    /// Like [`Cabinet::frame_2d_fade_in`], but for the shelf specifically
    /// (plan revision): reserves the panel column like `frame_shelf` does,
    /// and draws it flat, in the same viewport, right after the blended
    /// grid/static — the panel itself never fades with the signal, same as
    /// the in-game panel doesn't.
    pub fn frame_shelf_fade_in<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        static_level: f32,
        shelf_alpha: f32,
    ) {
        self.paint_shelf(bg, draw);
        self.update_noise_tex(static_level);
        let canvas_rect = self.canvas_rect;
        let panel = panel_rect(canvas_rect.width(), canvas_rect.height());

        let mesh_static = self.mesh_2d();
        let mesh_shelf = build_crt_mesh(self.screen, shelf_alpha.clamp(0.0, 1.0));

        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(canvas_rect));

        let nt = self.noise_tex.take().unwrap();
        let _ = self.canvas.render_geometry(
            &mesh_static.verts,
            Some(&nt.tex),
            &mesh_static.indices[..],
        );
        self.noise_tex = Some(nt);

        let st = self.screen_tex.take().unwrap();
        let _ =
            self.canvas
                .render_geometry(&mesh_shelf.verts, Some(&st.tex), &mesh_shelf.indices[..]);
        self.screen_tex = Some(st);

        let bezel = self.bezel.take().unwrap();
        self.draw_tv_glass_before_bezel(&mesh_static);
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.bezel = Some(bezel);
        draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            canvas_rect.height(),
            &self.nameplate,
            self.nameplate_updates,
        );
        self.shelf_buttons = draw_shelf_panel(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.shelf_panel.as_ref(),
            panel,
        );
        self.close_button = draw_close_button(&mut self.canvas, &mut self.font);
        self.minimize_button = draw_minimize_button(&mut self.canvas, &mut self.font);

        self.canvas.set_viewport(None);
        self.present_tube();
    }

    /// Fill the noise texture for the current window size at `level` (1.0 =
    /// full blizzard, 0.0 = a dim near-still hiss); leaves it in `noise_tex`.
    pub(super) fn update_noise_tex(&mut self, level: f32) {
        const NW: u32 = 320;
        const NH: u32 = 240;
        // The hiss regenerates at ~30Hz (and whenever the level moves) —
        // 60Hz of fresh 307KB noise + texture upload is invisible busywork.
        let now = Instant::now();
        if let Some((t, l)) = self.noise_stamp {
            if now.duration_since(t) < Duration::from_millis(30) && (l - level).abs() < 0.005 {
                return;
            }
        }
        self.noise_stamp = Some((now, level));
        self.ensure_noise_tex(NW, NH);
        // Sem sinal na entrada composta = a tela azul famosa da TV ("AV 1").
        // O level ainda governa o brilho: os bursts de power piscam a tela
        // mais clara antes de assentar no azul escuro de repouso.
        {
            let Self { noise, .. } = &mut *self;
            if noise.len() != (NW * NH * 4) as usize {
                *noise = vec![0u8; (NW * NH * 4) as usize];
            }
            let k = level.clamp(0.0, 1.0);
            // Azul de AV: quase nada de vermelho, verde pela metade.
            let (r, g, b) = (8u32, 32u32, (60.0 + 140.0 * k) as u32);
            let px = [r as u8, g as u8, b.min(255) as u8, 255];
            for p in noise.as_chunks_mut::<4>().0 {
                *p = px;
            }
        }
        let nt = self.noise_tex.as_mut().unwrap();
        let _ = nt.tex.update(None, &self.noise, (NW * 4) as usize);
    }

    /// The back cover enlarged (plan revision: "ao clicar no back cover
    /// possibilitar mostrar em tamanho maior"): the image aspect-preserved as
    /// large as the *tube* allows over a dark backdrop, with a "voltar" button
    /// centred right below it — drawn into the screen buffer and warped
    /// through the CRT mesh like the shelf itself, so the zoom happens on the
    /// TV, not flat over the whole window (plan revision: "abrir ele dentro
    /// da tv"). The flat side panel keeps drawing beside it, untouched.
    /// Returns the button's rect in screen-local coordinates — hit-test it
    /// through `hit_screen_point`, not raw output coordinates.
    pub fn frame_image_zoom(&mut self, id: u64, label: &str) -> Rect {
        let (sw, sh) = self.shelf_screen_size();
        let pad = 24i32;
        let btn_h = GLYPH_H as i32 + 14;
        let btn_w = (GLYPH_W as i32 * label.chars().count() as i32) + 28;
        let btn = Rect::new(
            (sw as i32 - btn_w).max(pad) / 2,
            sh as i32 - pad - btn_h,
            btn_w as u32,
            btn_h as u32,
        );
        let img_w = sw.saturating_sub(pad as u32 * 2);
        let img_h = (sh as i32 - pad * 2 - btn_h - 14).max(60) as u32;
        self.paint_shelf((10, 10, 11), |d| {
            d.image_fit(id, pad, pad, img_w, img_h);
            d.outline(
                btn.x(),
                btn.y(),
                btn.width(),
                btn.height(),
                1,
                (PANEL_TEXT.0, PANEL_TEXT.1, PANEL_TEXT.2, 255),
            );
            d.fill(
                btn.x() + 1,
                btn.y() + 1,
                btn.width().saturating_sub(2),
                btn.height().saturating_sub(2),
                (PANEL_BTN_BG.0, PANEL_BTN_BG.1, PANEL_BTN_BG.2, 255),
            );
            let text_w = GLYPH_W as i32 * label.chars().count() as i32;
            d.text(
                btn.x() + (btn.width() as i32 - text_w) / 2,
                btn.y() + (btn.height() as i32 - GLYPH_H as i32) / 2,
                1,
                PANEL_TEXT,
                label,
            );
        });
        self.composite_shelf();
        self.present_tube();
        btn
    }

    /// Render `draw` into the screen buffer. Shared by `frame_2d` / `capture_2d`.
    pub(super) fn paint_2d<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        self.canvas_rect = cabinet_canvas_rect(real_w, real_h);
        let ww = self.canvas_rect.width();
        self.paint_2d_avail(bg, draw, ww);
    }

    /// Like [`Cabinet::paint_2d`], but reserving the flat side-panel column
    /// first (plan revision: "o painel nao pode estar dentro da TV") — the
    /// shelf's own screen buffer only ever spans the narrower grid area.
    pub(super) fn paint_shelf<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        self.canvas_rect = cabinet_canvas_rect(real_w, real_h);
        let (out_w, out_h) = (self.canvas_rect.width(), self.canvas_rect.height());
        let cab_w = out_w.saturating_sub(panel_rect(out_w, out_h).width());
        self.paint_2d_avail(bg, draw, cab_w);
    }

    /// Shared by `paint_2d`/`paint_shelf`: `self.canvas_rect` must already be
    /// set; `avail_w` is how much of its width the screen buffer gets (all
    /// of it for the plain 2D path, minus the panel for the shelf's).
    pub(super) fn paint_2d_avail<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        avail_w: u32,
    ) {
        let wh = self.canvas_rect.height();
        self.screen = screen_area(avail_w, wh);
        let (sw, sh) = (self.screen.width(), self.screen.height());
        self.ensure_screen_tex(sw, sh);
        self.ensure_bezel(avail_w, wh, self.screen);

        let Self {
            canvas,
            screen_tex,
            font,
            images,
            ..
        } = self;
        let images = &*images;
        let st = screen_tex.as_mut().unwrap();
        let _ = canvas.with_texture_canvas(&mut st.tex, |c| {
            c.set_draw_color(Color::RGB(bg.0, bg.1, bg.2));
            c.clear();
            let mut s = Screen {
                canvas: c,
                font,
                images,
                w: sw,
                h: sh,
            };
            draw(&mut s);
        });
        // The clip lives on the shared underlying renderer; clear it.
        self.canvas.set_clip_rect(ClippingRect::None);
    }

    /// Warp the screen buffer through the tube into the live window, then frame.
    pub(super) fn composite_screen(&mut self) {
        let mesh = self.mesh_2d();
        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(self.canvas_rect));
        let st = self.screen_tex.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&mesh.verts, Some(&st.tex), &mesh.indices[..]);
        self.screen_tex = Some(st);
        self.draw_tv_glass_before_bezel(&mesh);
        self.mesh_2d = Some(mesh);
        let bezel = self.bezel.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.bezel = Some(bezel);
        draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            self.canvas_rect.height(),
            &self.nameplate,
            self.nameplate_updates,
        );
        self.draw_chin_persistent();
        self.close_button = draw_close_button(&mut self.canvas, &mut self.font);
        self.minimize_button = draw_minimize_button(&mut self.canvas, &mut self.font);
        self.canvas.set_viewport(None);
    }

    /// Like [`Cabinet::composite_screen`], but also draws the settings
    /// screen's flat panel (plan revision) right after the warped content —
    /// the section buttons and "Voltar", hit-tested via
    /// `hit_settings_button`.
    pub(super) fn composite_settings(&mut self) {
        self.composite_screen();
        let panel = panel_rect(self.canvas_rect.width(), self.canvas_rect.height());
        self.canvas.set_viewport(Some(self.canvas_rect));
        self.settings_buttons = draw_settings_panel(
            &mut self.canvas,
            &mut self.font,
            self.settings_panel.as_ref(),
            panel,
        );
        self.canvas.set_viewport(None);
    }

    /// Like [`Cabinet::frame_shelf`], but for the settings screen (plan
    /// revision: "a tela de configurações não está no padrão do resto do
    /// app") — the exact same split as the shelf (warped tube on the left,
    /// flat panel on the right), so the TV keeps the same proportions
    /// everywhere.
    pub fn frame_settings<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        self.paint_shelf(bg, draw);
        self.composite_settings();
        self.present_tube();
    }

    /// Like [`Cabinet::frame_2d`], but in the idle screen's own layout (plan
    /// revision: "na tela de download fazer igual o padrão das outras telas,
    /// não esticar a tv e mostrar o painel lateral") — the screen buffer
    /// only spans the cabinet column (same split as the shelf/settings), so
    /// the TV keeps its normal proportions, and the idle side panel draws
    /// and stays clickable around it. Hit-testing inside `draw` pairs with
    /// `window_to_screen`.
    pub fn frame_idle_2d<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        self.paint_shelf(bg, draw);
        self.composite_screen();
        let panel = panel_rect(self.canvas_rect.width(), self.canvas_rect.height());
        self.canvas.set_viewport(Some(self.canvas_rect));
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
        self.canvas.set_viewport(None);
        self.present_tube();
    }

    /// Like [`Cabinet::capture_shelf`], but drawing the settings panel
    /// (headless preview — the BMP matches what a real window would show).
    pub fn capture_settings<F: FnOnce(&mut Screen)>(
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
        let settings_panel = self.settings_panel.as_ref();
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
            let _ = draw_settings_panel(c, font, settings_panel, panel);
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

    /// Like [`Cabinet::frame_idle_2d`], but composited into an offscreen
    /// target and saved as a BMP (headless preview — the BMP matches what a
    /// real window would show, idle panel included).
    pub fn capture_idle_2d<F: FnOnce(&mut Screen)>(
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
        let panel_info = self.panel.as_ref();
        let session = self.session;
        let idle_core_prompt = self.idle_core_prompt.as_deref();
        let dev_mode = self.dev_mode;
        let core_status = self.core_status.as_deref();
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
            let _ = draw_panel(
                c,
                font,
                images,
                panel_info,
                0.0,
                panel,
                session,
                idle_core_prompt,
                dev_mode,
                core_status,
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

    /// Like [`Cabinet::composite_screen`], but also draws the shelf's flat
    /// side panel (plan revision) right after the warped grid, in the same
    /// viewport — undistorted, like the in-game panel already is.
    pub(super) fn composite_shelf(&mut self) {
        self.composite_screen();
        let panel = panel_rect(self.canvas_rect.width(), self.canvas_rect.height());
        self.canvas.set_viewport(Some(self.canvas_rect));
        self.shelf_buttons = draw_shelf_panel(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.shelf_panel.as_ref(),
            panel,
        );
        self.canvas.set_viewport(None);
    }

    pub(super) fn ensure_screen_tex(&mut self, w: u32, h: u32) {
        if matches!(&self.screen_tex, Some(s) if s.w == w && s.h == h) {
            return;
        }
        let mut tex = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, w, h)
            .expect("create screen target");
        tex.set_scale_mode(SdlScaleMode::Linear);
        // So `frame_2d_fade_in`'s vertex alpha can blend it over the snow.
        tex.set_blend_mode(BlendMode::Blend);
        self.screen_tex = Some(SizedTex { tex, w, h });
    }

    pub(super) fn ensure_noise_tex(&mut self, w: u32, h: u32) {
        if matches!(&self.noise_tex, Some(s) if s.w == w && s.h == h) {
            return;
        }
        let mut tex = self
            .canvas
            .create_texture_streaming(SdlFormat::RGBA32, w, h)
            .expect("create noise texture");
        tex.set_scale_mode(SdlScaleMode::Linear);
        self.noise_tex = Some(SizedTex { tex, w, h });
    }
}
