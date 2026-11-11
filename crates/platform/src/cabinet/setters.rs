//! Gabinete: setters (extraído de `cabinet/mod.rs`, sem mudança de comportamento).

use super::*;

impl Cabinet {
    /// Replace the cabinet's nameplate text (plan revision) — the app calls
    /// this once at startup with its own version, and again whenever the
    /// installed PCSX Rearmed core changes (a download/update via settings).
    pub fn set_nameplate(&mut self, text: &str) {
        self.nameplate = text.to_string();
    }

    /// Which nameplate lines get the green "tem update" dot — `(app, core)`,
    /// matching the block `set_nameplate` stacked (`draw_brand` draws the dot
    /// right after line 0 / line 1, respectively). `false, false` clears both.
    pub fn set_nameplate_updates(&mut self, app: bool, core: bool) {
        self.nameplate_updates = (app, core);
    }

    /// Qual seta verde de update do nameplate um ponto (output) acerta —
    /// rects preenchidos pelo caminho estático (`present_static`), onde o
    /// nameplate é clicável (idle). (plan revision: "ao clicar no icone
    /// verde de atualizar".)
    pub fn hit_update_arrow(&self, ox: i32, oy: i32) -> Option<UpdateArrow> {
        let (app, core) = self.update_arrows;
        if app.is_some_and(|r| r.contains_point((ox, oy))) {
            Some(UpdateArrow::App)
        } else if core.is_some_and(|r| r.contains_point((ox, oy))) {
            Some(UpdateArrow::Core)
        } else {
            None
        }
    }

    /// MOCK (design preview for the achievements notification — see
    /// `demo_osd`): shows a line block in the chin's right side; an empty
    /// slice clears it.
    /// Arm/disarm the persistent RetroAchievements chin badge — the runner
    /// calls it with the live session's mode on cart insert and with `None`
    /// when the cartridge leaves (or the account is off). No visual effect
    /// while the OSD queue has a live notification: that draws first.
    pub fn set_ra_status(&mut self, status: Option<RaStatus>) {
        self.ra_status = status;
    }

    pub fn push_osd(&mut self, lines: &[&str], badge: Option<u64>, ttl: Duration) {
        if lines.is_empty() {
            return;
        }
        self.osd_queue.push_back(OsdEntry {
            lines: lines.iter().map(|l| l.to_string()).collect(),
            badge,
            until: Instant::now() + ttl,
        });
    }

    /// The idle panel's core prompt (plan revision: "avisar que para jogar é
    /// necessário o download do core") — `Some(button_label)` shows the
    /// warning line plus a "CoreDownload" button; `None` hides both. Only
    /// ever rendered on the idle (panel-less) screen.
    pub fn set_idle_core_prompt(&mut self, label: Option<&str>) {
        self.idle_core_prompt = label.map(|l| l.to_string());
    }

    /// O modo dev (plan revision: "konami code na tela inicial do app") —
    /// ligado só pelo app, ao reconhecer o código; enquanto ligado, o painel
    /// da tela inicial desenha o botão "Dev" acima do "Configurações".
    /// Nunca é persistido nem lido de arquivo: o segredo vive só na sessão.
    pub fn set_dev_mode(&mut self, on: bool) {
        self.dev_mode = on;
    }

    /// O estado atual do modo dev — o app consulta para saber se a sessão
    /// já o desbloqueou (o botão em si o Cabinet desenha sozinho).
    pub fn dev_mode(&self) -> bool {
        self.dev_mode
    }

    /// A linha de status do download do núcleo no painel do idle ("baixando…
    /// X MB" / "núcleo atualizado!" / motivo da falha) — `None` não desenha
    /// nada. O app atualiza a cada frame enquanto o download vive.
    pub fn set_core_status(&mut self, status: Option<&str>) {
        self.core_status = status.map(|s| s.to_string());
    }

    pub fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
        let _ = self.canvas.window_mut().set_fullscreen(self.fullscreen);
    }

    /// The underlying window — `Platform::start_text_input`/`stop_text_input`
    /// need it (SDL's text-input API is per-window).
    pub(crate) fn window(&self) -> &sdl3::video::Window {
        self.canvas.window()
    }

    /// Convert a click's window coordinates (what SDL reports) into the
    /// cabinet's own local space — what `panel_rect`/`draw_panel`/`screen`
    /// all lay out in. Two corrections stack here: window → real output
    /// pixels (they differ on a HiDPI display), then output → cabinet-local
    /// (subtracting `canvas_rect`'s offset — on an ultrawide window the
    /// cabinet is letterboxed, not the whole output). A point outside
    /// `canvas_rect` (in the letterbox bars) still comes back as a
    /// coordinate — callers bounds-check against their own rects, e.g.
    /// `hit_panel_button`/`hit_screen_point`.
    pub fn window_to_output(&self, x: i32, y: i32) -> (i32, i32) {
        let (ww, wh) = self.canvas.window().size();
        let (ow, oh) = self.canvas.output_size().unwrap_or((ww, wh));
        let sx = ow as f32 / ww.max(1) as f32;
        let sy = oh as f32 / wh.max(1) as f32;
        let (ox, oy) = ((x as f32 * sx) as i32, (y as f32 * sy) as i32);
        (ox - self.canvas_rect.x(), oy - self.canvas_rect.y())
    }

    /// The active renderer's name (the boot log line's same value) — the
    /// slow-motion watchdog logs it so a Deck report pins the renderer
    /// without asking for the whole boot log.
    pub fn renderer_name(&self) -> &str {
        &self.canvas.renderer_name
    }

    /// Liga/desliga o painel de desempenho do modo dev (o menu dev alterna;
    /// o estado mora aqui e sobrevive estante ↔ jogo).
    pub fn set_perf_overlay(&mut self, on: bool) {
        self.perf_overlay = on;
    }

    /// O painel de desempenho está ligado? (o menu dev desenha o botão no
    /// estado atual.)
    pub fn perf_overlay(&self) -> bool {
        self.perf_overlay
    }

    /// Alimenta o painel: fps medido pelo runner (janela de 1 s) e as
    /// leituras de sistema (°C, MB — arredondados; `None` = leitura
    /// indisponível, o segmento sai da linha).
    pub fn set_perf_readout(&mut self, fps: u32, cpu: Option<f32>, ram_mb: Option<f32>) {
        self.perf_fps = fps;
        self.perf_cpu = cpu.map(|c| c.round() as u32);
        self.perf_ram_mb = ram_mb.map(|m| m.round() as u32);
    }

    /// Like [`Cabinet::window_to_output`], but in the 2D screen buffer's own
    /// coordinate space (what `frame_2d`'s `draw` closure paints in — the
    /// screen is inset by the bezel inside the cabinet canvas). A screen
    /// drawn with hand-laid buttons hit-tests clicks by pairing this with
    /// its `Screen::size` layout.
    pub fn window_to_screen(&self, x: i32, y: i32) -> (i32, i32) {
        let (ox, oy) = self.window_to_output(x, y);
        (ox - self.screen.x(), oy - self.screen.y())
    }

    /// Show/hide the top-left "fechar app" button (plan revision) — call
    /// once on entering a screen that should offer it (idle/shelf/settings/
    /// history), and with `false` on entering one that shouldn't
    /// (gameplay) — the flag has no default that fits every screen, so it's
    /// never implicitly reset between them.
    pub fn set_close_button(&mut self, _show: bool) {
        // The fechar/minimizar pair is drawn on every screen now (plan
        // revision); kept as a no-op so existing callers keep compiling.
    }

    /// Whether an output-space point lands on the close button drawn last
    /// frame — always `false` while `set_close_button(false)` is in effect,
    /// even if a stale rect from an earlier screen is still sitting in
    /// `close_button`. Coordinates from a click go through
    /// `window_to_output` first.
    pub fn hit_close_button(&self, out_x: i32, out_y: i32) -> bool {
        self.close_button.contains_point((out_x, out_y))
    }

    /// Whether a click (already mapped by `window_to_output`) landed on the
    /// top-left minimize button. Screens call `minimize()` when this hits.
    pub fn hit_minimize_button(&self, out_x: i32, out_y: i32) -> bool {
        self.minimize_button.contains_point((out_x, out_y))
    }

    /// Iconify the window (plan revision: "botão de minimizar ao lado de
    /// fechar") — the app keeps running; restoring is the user's click on
    /// the Dock/taskbar, like any OS window.
    pub fn minimize(&mut self) {
        let _ = self.canvas.window_mut().minimize();
    }

    /// Which panel button, if any, sits under an output-space point — the
    /// idle screen's "Inserir cartucho" or one of the in-game commands drawn
    /// last frame. Coordinates from a click go through `window_to_output`
    /// first.
    /// Aponta o foco do controle para um botão (anel em volta dele, no
    /// próximo `present`); `None` apaga. Vale por poucos quadros — a tela
    /// dona chama a cada quadro.
    pub fn set_pad_focus(&mut self, focus: Option<PadFocus>) {
        self.pad_focus = focus.map(|f| (f, PAD_FOCUS_TTL));
    }

    /// Os botões do painel da tela inicial desenhados no último quadro, em
    /// ordem de leitura (de cima para baixo, esquerda para a direita) — o
    /// que a navegação por controle percorre.
    pub fn panel_button_list(&self) -> Vec<PanelButton> {
        let mut v: Vec<&(PanelButton, Rect)> = self.panel_buttons.iter().collect();
        v.sort_by_key(|(_, r)| (r.y(), r.x()));
        v.into_iter().map(|(b, _)| *b).collect()
    }

    pub fn hit_panel_button(&self, out_x: i32, out_y: i32) -> Option<PanelButton> {
        self.panel_buttons
            .iter()
            .find(|(_, r)| r.contains_point((out_x, out_y)))
            .map(|(b, _)| *b)
    }

    /// Same as `hit_panel_button`, for the pause book's own buttons — a
    /// separate list since the pause screen replaces the whole window
    /// instead of sharing it with the side panel (see `pause_buttons`).
    pub fn hit_pause_button(&self, out_x: i32, out_y: i32) -> Option<PanelButton> {
        self.pause_buttons
            .iter()
            .find(|(_, r)| r.contains_point((out_x, out_y)))
            .map(|(b, _)| *b)
    }

    /// Same as `hit_panel_button`, for the shelf's own flat panel (plan
    /// revision) — a separate list since the shelf isn't a loaded game.
    pub fn hit_shelf_button(&self, out_x: i32, out_y: i32) -> Option<ShelfButton> {
        self.shelf_buttons
            .iter()
            .find(|(_, r)| r.contains_point((out_x, out_y)))
            .map(|(b, _)| *b)
    }

    /// Show the shelf's flat side panel content for whatever game is
    /// selected right now (plan revision) — rebuilt every frame the
    /// selection might have changed, same as the shelf already rebuilds its
    /// own grid draw closure; the struct is small enough that this is cheap.
    pub fn set_shelf_panel(&mut self, info: ShelfPanelInfo) {
        self.shelf_panel = Some(info);
    }

    /// Settings screen's flat panel content — see [`SettingsPanelInfo`]
    /// (plan revision: settings now lives on the shelf's layout, tube +
    /// panel, and the panel carries the section buttons).
    pub fn set_settings_panel(&mut self, info: SettingsPanelInfo) {
        self.settings_panel = Some(info);
    }
    pub fn hit_settings_button(&self, out_x: i32, out_y: i32) -> Option<SettingsButton> {
        self.settings_buttons
            .iter()
            .find(|(_, r)| r.contains_point((out_x, out_y)))
            .map(|(b, _)| *b)
    }

    /// Map an output/canvas-space click into the 2D screen buffer's local
    /// coordinates — what `Screen::fill`/`text`/`image_fit` see, e.g. in the
    /// shelf's grid (plan §3.1). `None` outside the tube. Approximate: the
    /// buffer is warped onto the tube by `build_crt_mesh` (a mild barrel bow,
    /// `CRT_WARP`), which this ignores — a click right at the curved edge can
    /// land a few px off, but shelf tiles sit well inside it.
    pub fn hit_screen_point(&self, out_x: i32, out_y: i32) -> Option<(i32, i32)> {
        let (lx, ly) = (out_x - self.screen.x(), out_y - self.screen.y());
        if lx < 0 || ly < 0 || lx >= self.screen.width() as i32 || ly >= self.screen.height() as i32
        {
            return None;
        }
        let (sw, sh) = (self.screen.width() as f32, self.screen.height() as f32);
        // Displaced grid coords the click arrived at (-1..1 per axis).
        let dx = (lx as f32 / sw) * 2.0 - 1.0;
        let dy = (ly as f32 / sh) * 2.0 - 1.0;
        // Recover the undisplaced ones: forward is cx' = cx*(1-W*cy^2),
        // cy' = cy*(1-W*cx^2) — divide out the partner axis' factor until it
        // stops moving (sub-pixel after three rounds; eight for free).
        // (o ganho de `crt_displace` sai primeiro; as bordas presas ficam nelas.)
        let gain = 1.0 / (1.0 - CRT_WARP);
        let (dx, dy) = (dx / gain, dy / gain);
        let (mut cx, mut cy) = (dx, dy);
        for _ in 0..8 {
            cy = dy / (1.0 - CRT_WARP * cx * cx);
            cx = dx / (1.0 - CRT_WARP * cy * cy);
        }
        if !(-1.0..=1.0).contains(&cx) || !(-1.0..=1.0).contains(&cy) {
            // In the recess between the warped picture's corner and the
            // bezel — nothing drawn there to click.
            return None;
        }
        Some((
            (((cx * 0.5 + 0.5) * sw) as i32).clamp(0, sw as i32 - 1),
            (((cy * 0.5 + 0.5) * sh) as i32).clamp(0, sh as i32 - 1),
        ))
    }

    /// Show the side panel during play: `logo` (width, height, RGBA) is the
    /// local art if there is one, else the panel falls back to `title` in
    /// text (plan §3.2, item 1); `cartridge` is a second, independent local
    /// image drawn right below it when present (plan revision — neither
    /// needs the other). `commands` is the initial button legend (item 3,
    /// plan revision: mouse-only) — see `set_commands` for the per-frame
    /// updates that follow (labels like a "(feito!)" flash change live).
    /// Call once per game.
    pub fn set_panel(
        &mut self,
        logo: Option<(u32, u32, &[u8])>,
        cartridge: Option<(u32, u32, &[u8])>,
        title: &str,
        commands: &[(PanelButton, String, bool)],
    ) {
        let has_logo = if let Some((w, h, rgba)) = logo {
            self.set_image(PANEL_LOGO_IMG, w, h, rgba);
            true
        } else {
            false
        };
        let has_cartridge = if let Some((w, h, rgba)) = cartridge {
            self.set_image(PANEL_CARTRIDGE_IMG, w, h, rgba);
            true
        } else {
            false
        };
        self.panel = Some(PanelInfo {
            has_logo,
            has_cartridge,
            title: title.to_string(),
            commands: commands.to_vec(),
            // A freshly inserted cartridge (plan revision): the console
            // doesn't boot itself any more — see `run_game`'s initial
            // `powered = false` — so the rocker starts down, not up.
            powered: false,
            lid_open: false,
            disc_in: true,
            reset_pressed: false,
            card1_label: None,

            analog1: false,
            cheats: Vec::new(),
            note_count: 0,
            has_note_thumb: false,
            text_note: None,
            // Seated in the slot until `runner`'s insert animation says
            // otherwise (it runs right after this).
            cartridge_motion: None,
            card_motion: None,
        });
    }

    /// Refresh the command legend's labels (plan revision: a "(feito!)"
    /// flash on a silent action is live state, so the caller rebuilds and
    /// passes this every frame instead of once). A no-op before `set_panel`.
    pub fn set_commands(&mut self, commands: &[(PanelButton, String, bool)]) {
        if let Some(panel) = &mut self.panel {
            panel.commands = commands.to_vec();
        }
    }

    /// Dim/brighten the command buttons to match the console's power state
    /// (Eject only clunks while powered, Reset only acts while powered) — see
    /// `PanelInfo::powered`. A no-op before `set_panel`.
    pub fn set_powered(&mut self, powered: bool) {
        if let Some(panel) = &mut self.panel {
            panel.powered = powered;
        }
    }

    /// Estado do drive: tampa aberta/fechada e disco presente/ausente —
    /// governa o giro e o vidro da tampa. O disco assentado pela estante
    /// não sai do drive (não há remoção/troca no painel).
    pub fn set_drive(&mut self, lid_open: bool, disc_in: bool) {
        if let Some(p) = self.panel.as_mut() {
            p.lid_open = lid_open;
            p.disc_in = disc_in;
        }
    }

    /// O LED do botão ANALOG da entrada 1 — o do controle original:
    /// vermelho = desligado (digital), verde = ligado (analógico +
    /// vibração). Sem painel de jogo aberto, é no-op.
    pub fn set_analog_led(&mut self, on: bool) {
        if let Some(p) = self.panel.as_mut() {
            p.analog1 = on;
        }
    }

    /// O nome do memory card encaixado, impresso no painel do adesivo da
    /// porta (plan revision: "mostrar o nome dele no espacinho que fica
    /// acima da copy no slot escolhido"). `None` deixa o painel liso. Sem
    /// painel de jogo aberto, é no-op.
    pub fn set_card_labels(&mut self, card1: Option<&str>) {
        if let Some(p) = self.panel.as_mut() {
            p.card1_label = card1.map(|s| s.to_string());
        }
    }

    /// Reset's rocker springs up for a short moment after a click, then back
    /// down on its own (plan revision) — the caller (`runner`) recomputes
    /// this every frame from its own click timestamp, same pattern as the
    /// "(feito!)" flash on the other buttons.
    pub fn set_reset_pressed(&mut self, pressed: bool) {
        if let Some(panel) = &mut self.panel {
            panel.reset_pressed = pressed;
        }
    }

    /// Drop the last game's panel (logo, commands, cheats, notes, clock) —
    /// call on the way back to the idle/root screen, so `draw_panel` shows
    /// the "Inserir cartucho" button instead of the previous game's stale
    /// info. A no-op if there's nothing set.
    pub fn clear_panel(&mut self) {
        self.panel = None;
    }

    /// The idle screen's console-brand logo (plan revision: the idle/root
    /// screen — startup, backing out of the shelf, or ejecting a game, all
    /// the same screen — shows this where a game's own logo would go).
    /// `assets/console.png`, decoded once by the caller (`idle::run`); a
    /// no-op with `None` (the file doesn't exist) — `draw_panel`'s idle
    /// branch falls back to plain text the same way a missing per-game logo
    /// does.
    pub fn set_console_logo(&mut self, logo: Option<(u32, u32, &[u8])>) {
        if let Some((w, h, rgba)) = logo {
            self.set_image(PANEL_CONSOLE_LOGO_IMG, w, h, rgba);
        }
    }

    /// The console tag wordmark printed on the slot's loading base (plan
    /// revision: "imagem console-tag... na base do cartucho, alinhado a
    /// esquerda") — `assets/console-tag.png`, loaded once by the app and
    /// drawn by `draw_slot_furniture` on every screen that shows the slot.
    /// `None` removes a previously loaded tag (file gone), leaving the bare
    /// groove line.
    /// A imagem do leitor de CD (foto real, mascarada em círculo) — o
    /// placeholder que fica sob o disco girando. Sempre presente.
    pub fn set_cd_reader(&mut self, img: Option<(u32, u32, &[u8])>) {
        match img {
            Some((w, h, rgba)) => self.set_image(CD_READER_IMG, w, h, rgba),
            None => {
                self.images.remove(&CD_READER_IMG);
            }
        }
    }

    /// A mesa do spindle (onde o CD assenta): desenhada no eixo do leitor
    /// com o MESMO ângulo do disco — aparece pelo furo central do CD e
    /// durante a ejeção. Sem ela, o furo mostra o poço parado.
    pub fn set_cd_seat(&mut self, img: Option<(u32, u32, &[u8])>) {
        match img {
            Some((w, h, rgba)) => self.set_image(CD_SEAT_IMG, w, h, rgba),
            None => {
                self.images.remove(&CD_SEAT_IMG);
            }
        }
    }

    /// O vidro da tampa de acrílico: reflexos com gradiente pré-renderizados
    /// (RGBA, blend por alpha no `copy`). `None` remove.
    pub fn set_lid_glass(&mut self, img: Option<(u32, u32, &[u8])>) {
        match img {
            Some((w, h, rgba)) => self.set_image(LID_GLASS_IMG, w, h, rgba),
            None => {
                self.images.remove(&LID_GLASS_IMG);
            }
        }
    }

    pub fn set_slot_tag(&mut self, tag: Option<(u32, u32, &[u8])>) {
        match tag {
            Some((w, h, rgba)) => self.set_image(SLOT_TAG_IMG, w, h, rgba),
            None => {
                self.images.remove(&SLOT_TAG_IMG);
            }
        }
    }

    /// Update the panel's notebook block (plan §3.4, item 5): `count` of the
    /// 15 print slots that are pinned (plan revision — used to be however
    /// many were filled, shown regardless of pin; now the panel only
    /// features what the player deliberately pinned), `thumb` a pinned
    /// slot's image (width, height, RGBA) if there's one to show, `text` a
    /// pinned text-note slot's content if there's one of those to show —
    /// independent of the photo side, so either, both, or neither can be
    /// present at once. Call once at game start and again after every
    /// capture, write, or pin toggle (either kind). A no-op before
    /// `set_panel`.
    pub fn set_notes(
        &mut self,
        count: usize,
        thumb: Option<(u32, u32, &[u8])>,
        text: Option<&str>,
    ) {
        let has_thumb = if let Some((w, h, rgba)) = thumb {
            self.set_image(PANEL_NOTE_IMG, w, h, rgba);
            true
        } else {
            false
        };
        if let Some(panel) = &mut self.panel {
            panel.note_count = count;
            panel.has_note_thumb = has_thumb;
            panel.text_note = text.map(str::to_string);
        }
    }

    /// Update the side panel's cheat list (plan §4.4): `cheats` is
    /// `(description, on)` pairs in the curated order, shown informationally
    /// here (only the ones that are on, plain text) — toggling lives in the
    /// Cheats modal now (plan revision: its own menu, not a page inside the
    /// notebook). Call once at game start and again on every toggle — the
    /// list is always tiny. A no-op before `set_panel`.
    pub fn set_cheats(&mut self, cheats: &[(String, bool)]) {
        if let Some(panel) = &mut self.panel {
            panel.cheats = cheats.to_vec();
        }
    }

    /// Update the session clock shown at the bottom of the panel. Call once a
    /// frame; it just stores the value for the next present/capture.
    pub fn set_session_time(&mut self, elapsed: Duration) {
        self.session = elapsed;
    }

    /// Load the pause book's content (plan §3.2/§3.4): call once when pause
    /// opens, not every frame — `present_pause` just redraws what's already
    /// set. `captures` is the fixed slot count shared by both pages (plan
    /// revision: always 15). Neither page has anything to show yet — follow
    /// with `set_pause_page` and `set_pause_text_page`.
    pub fn set_pause_note(&mut self, title: &str, captures: usize) {
        self.pause = Some(PauseNote {
            title: title.to_string(),
            captures,
            page: captures.saturating_sub(1),
            has_thumb: false,
            pinned: false,
            slot_label: String::new(),
            text_page: captures.saturating_sub(1),
            text_pinned: false,
            text_content: None,
            draft: None,
            draft_limit: 0,
            draft_heading: String::new(),
        });
    }

    /// Show a different capture on the right page (pagination, plan
    /// revision) — call on entering pause (page = the last one) and again on
    /// every Prev/Next/pin/rename change. `pinned`/`label` are that slot's
    /// current protection state and caption. A no-op before `set_pause_note`.
    pub fn set_pause_page(
        &mut self,
        page: usize,
        pinned: bool,
        label: &str,
        thumb: Option<(u32, u32, &[u8])>,
    ) {
        let has_thumb = if let Some((w, h, rgba)) = thumb {
            self.set_image(PAUSE_THUMB_IMG, w, h, rgba);
            true
        } else {
            false
        };
        if let Some(p) = &mut self.pause {
            p.page = page;
            p.has_thumb = has_thumb;
            p.pinned = pinned;
            p.slot_label = label.to_string();
        }
    }

    /// Show a different text-note slot on the left page (plan revision —
    /// mirrors `set_pause_page`, the right page's own pagination): call on
    /// entering pause and again on every Prev/Next/pin/write/delete change.
    /// `content` is `None` for an empty slot. A no-op before
    /// `set_pause_note`.
    pub fn set_pause_text_page(&mut self, page: usize, pinned: bool, content: Option<&str>) {
        if let Some(p) = &mut self.pause {
            p.text_page = page;
            p.text_pinned = pinned;
            p.text_content = content.map(str::to_string);
        }
    }

    /// Enter/update/leave an editor on the left page (plan revision: either
    /// the free-text note or a slot's caption — `heading` names which,
    /// shown above the live text): `Some(text)` shows `text` in place of the
    /// status view, with a `.../limit` counter; `None` leaves editing (any
    /// `heading` passed alongside `None` is ignored). Call on every
    /// keystroke while editing — cheap, at most a few hundred characters.
    /// A no-op before `set_pause_note`.
    pub fn set_pause_draft(&mut self, draft: Option<&str>, limit: usize, heading: &str) {
        if let Some(p) = &mut self.pause {
            p.draft = draft.map(str::to_string);
            p.draft_limit = limit;
            p.draft_heading = heading.to_string();
        }
    }

    /// Open (or replace) a save/load-state, print slot or card picker
    /// (plan revision): `title` names the dialog, `rows` is one `(label,
    /// enabled)` pair per slot in order — a disabled row (e.g. "Carregar"
    /// on an empty slot, or a pinned print slot) still shows, just dimmed
    /// and unclickable. Scroll position, `searchable`, and any active
    /// search filter all carry over from whatever modal was already open
    /// (Cheats calls this again after every toggle, to redraw its
    /// checkmarks, without wanting the view to jump back to the top or
    /// forget the filter) — only a genuinely fresh open (no modal was
    /// showing) starts scrolled to the top with no filter and not
    /// searchable; call `set_modal_searchable` right after opening one that
    /// should be.
    pub fn set_modal(&mut self, title: &str, rows: &[(String, bool)]) {
        let scroll = self.modal.as_ref().map_or(0, |m| m.scroll);
        self.modal = Some(ModalInfo {
            title: title.to_string(),
            rows: rows
                .iter()
                .map(|(label, enabled)| ModalRow {
                    label: label.clone(),
                    enabled: *enabled,
                    icon: None,
                })
                .collect(),
            scroll,
            draft: None,
            draft_limit: 0,
            draft_heading: String::new(),
        });
    }

    /// Como [`Cabinet::set_modal`], com um ícone 16×16 RGBA por linha — os
    /// memory cards listam os saves com o ícone do jogo gravado no bloco.
    /// Linhas com `None` saem sem ícone, como no modal de texto puro.
    #[allow(clippy::type_complexity)]
    pub fn set_modal_with_icons(
        &mut self,
        title: &str,
        rows: &[(String, bool, Option<(u32, u32, Vec<u8>)>)],
    ) {
        let scroll = 0;
        let built: Vec<ModalRow> = rows
            .iter()
            .map(|(label, enabled, icon)| ModalRow {
                label: label.clone(),
                enabled: *enabled,
                icon: icon.as_ref().and_then(|(w, h, rgba)| {
                    let mut tex = self
                        .canvas
                        .create_texture_static(SdlFormat::RGBA32, *w, *h)
                        .ok()?;
                    tex.update(None, rgba, (*w * 4) as usize).ok()?;
                    tex.set_blend_mode(BlendMode::Blend);
                    tex.set_scale_mode(SdlScaleMode::Nearest);
                    Some(tex)
                }),
            })
            .collect();
        self.modal = Some(ModalInfo {
            title: title.to_string(),
            rows: built,
            scroll,
            draft: None,
            draft_limit: 0,
            draft_heading: String::new(),
        });
    }

    /// Scroll the modal's row grid by `delta` visual rows (negative = up) —
    /// a no-op before `set_modal`. `draw_modal` clamps the visible result
    /// against however many rows actually overflow the card, so over-
    /// scrolling just stops at the last page rather than needing a bound
    /// here too.
    pub fn scroll_modal(&mut self, delta: i32) {
        if let Some(m) = &mut self.modal {
            m.scroll = (m.scroll as i32 + delta).max(0) as usize;
        }
    }

    /// Enter/update/leave the modal's naming step (plan revision — the print
    /// picker's second step, same idea as `set_pause_draft`): `Some(text)`
    /// replaces the row grid with `text` and a `.../limit` counter. A no-op
    /// before `set_modal`.
    pub fn set_modal_draft(&mut self, draft: Option<&str>, limit: usize, heading: &str) {
        if let Some(m) = &mut self.modal {
            m.draft = draft.map(str::to_string);
            m.draft_limit = limit;
            m.draft_heading = heading.to_string();
        }
    }

    /// Close whichever modal is open — call once the player picks a slot (or
    /// finishes naming a print) and the action's done, or on Cancel.
    pub fn clear_modal(&mut self) {
        self.modal = None;
    }

    /// Same as `hit_panel_button`, for the modal dialog's own buttons — a
    /// separate list since the modal replaces the whole window instead of
    /// sharing it with the side panel (see `modal_buttons`).
    pub fn hit_modal_button(&self, out_x: i32, out_y: i32) -> Option<PanelButton> {
        self.modal_buttons
            .iter()
            .find(|(_, r)| r.contains_point((out_x, out_y)))
            .map(|(b, _)| *b)
    }
}
