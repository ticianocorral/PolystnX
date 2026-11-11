//! Gabinete: panel (extraído de `cabinet.rs`, sem mudança de comportamento).

use super::*;

/// Draw the side panel: background, logo (or title) at top, session timer at
/// the bottom. Plain widgets, not warped by the tube (plan §2, §3.2). `None`
/// (no game loaded — the idle/root screen) draws just the "Inserir cartucho"
/// button in the logo's spot. Returns the clickable buttons drawn this frame,
/// in `rect`'s (output/canvas) coordinate space — the caller stores them for
/// `Cabinet::hit_panel_button`.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_panel(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    panel: Option<&PanelInfo>,
    spin: f32,
    rect: Rect,
    session: Duration,
    idle_core_prompt: Option<&str>,
    dev_mode: bool,
    core_status: Option<&str>,
) -> Vec<(PanelButton, Rect)> {
    if rect.width() == 0 {
        return Vec::new();
    }
    canvas.set_draw_color(Color::RGB(PANEL_BG.0, PANEL_BG.1, PANEL_BG.2));
    let _ = canvas.fill_rect(rect);

    let pad = 20i32;
    let inner_w = rect.width().saturating_sub(pad as u32 * 2);
    let x = rect.x() + pad;
    let y = rect.y() + pad;

    let Some(panel) = panel else {
        // Idle/root screen == the "cartridge ejected" screen (plan revision:
        // one screen, not two — startup, backing out of the shelf, and
        // ejecting a game all land here). Same slots a loaded game uses
        // (logo, then the cartridge block) with idle-appropriate stand-ins:
        // the console's own brand logo where a game's logo would sit, and
        // the cartridge block drawn as the same console slot — with an
        // "Inserir cartucho" button standing where a seated cartridge would
        // (plan revision: "na tela inicial... mostrar como se fosse a tela
        // do jogo, porem no lugar do cartucho inserido, mostrar botão
        // inserir cartucho"). No command legend — there's nothing loaded to
        // command — and Configuracoes moves to the footer, the same spot
        // the session clock uses during play.
        let mut cy = if images.contains_key(&PANEL_CONSOLE_LOGO_IMG) {
            draw_image_absolute(canvas, images, PANEL_CONSOLE_LOGO_IMG, x, y, inner_w, 110);
            y + 110
        } else {
            draw_text_wrapped_absolute(
                canvas,
                font,
                x,
                y,
                inner_w,
                TextStyle::new(2.0, PANEL_TEXT),
                BRAND,
            )
        };
        cy += 4;
        // O botão da estante flutuante no topo (estilo SNES, um pouco
        // maior) — e nada de console: a face do PSX vive só na tela de
        // jogo (plan revision: "remover o console da tela inicial"). O
        // resto da coluna fica limpo até o rodapé.
        let btn_w = (inner_w as f32 * 0.75).round() as u32;
        let btn_h = GLYPH_H * 2 + 24;
        let btn = Rect::new(
            x + (inner_w as i32 - btn_w as i32) / 2,
            cy + 6,
            btn_w,
            btn_h,
        );
        let insert_drawn = draw_button(canvas, font, btn, "Estante de games", true);
        cy = insert_drawn.bottom();

        let btn_h = (GLYPH_H + 12) as i32;
        let settings = Rect::new(x, rect.bottom() - pad - btn_h, inner_w, btn_h as u32);
        let mut buttons = vec![
            (PanelButton::Insert, insert_drawn),
            (
                PanelButton::Settings,
                draw_button(canvas, font, settings, "Configurações", true),
            ),
        ];
        // O manual do console: logo acima do "Configurações"; o Dev e o
        // aviso de core sobem junto.
        let manual = Rect::new(x, settings.y() - 8 - btn_h, inner_w, btn_h as u32);
        buttons.push((
            PanelButton::ConsoleManual,
            draw_button(canvas, font, manual, "Manual do console", true),
        ));
        // O botão do modo dev (plan revision: "habilitar botão em cima da
        // configuração") — só existe com o devmode ligado, e o devmode só
        // entra pelo código Konami nesta tela. Entra na pilha entre o
        // manual do console e o aviso de core, que sobe junto.
        let mut btn_top = manual.y();
        if dev_mode {
            let dev = Rect::new(x, btn_top - 8 - btn_h, inner_w, btn_h as u32);
            btn_top = dev.y();
            buttons.push((
                PanelButton::Dev,
                draw_button(canvas, font, dev, "Dev", true),
            ));
        }
        // The core prompt (plan revision: "avisar que para jogar é
        // necessário o download do core" / "coloque o aviso em cima do
        // botão de download"): warning line directly above the button, both
        // just above the Configurações footer (ou do Dev, quando ele existe).
        if let Some(label) = idle_core_prompt {
            const WARN: &str = "Para jogar é necessário baixar o núcleo PCSX Rearmed.";
            let btn = Rect::new(x, btn_top - 8 - btn_h, inner_w, btn_h as u32);
            let warn_h = wrapped_height(inner_w, 1, WARN);
            let warn_y = btn.y() - 6 - warn_h;
            if warn_y > cy {
                draw_text_wrapped_absolute(
                    canvas,
                    font,
                    x,
                    warn_y,
                    inner_w,
                    TextStyle::new(1.0, PANEL_WARN),
                    WARN,
                );
                buttons.push((
                    PanelButton::CoreDownload,
                    draw_button(canvas, font, btn, label, true),
                ));
            }
        } else if let Some(status) = core_status {
            // Andamento do download do núcleo na seta verde (plan revision:
            // "cliquei no icone verde de update do nucleo mas nao abriu
            // nada") — linha dim acima do rodapé; sem botão, o clique é na
            // seta do nameplate.
            let warn_h = wrapped_height(inner_w, 1, status);
            let warn_y = btn_top - 6 - warn_h;
            if warn_y > cy {
                draw_text_wrapped_absolute(
                    canvas,
                    font,
                    x,
                    warn_y,
                    inner_w,
                    TextStyle::new(1.0, PANEL_WARN),
                    status,
                );
            }
        }
        return buttons;
    };

    // 1. Logo, or the title if there isn't one (plan §3.2, item 1).
    // Steam Deck (painel < 360 px úteis): a logo reserva menos altura — é o
    // console que sobe; FHD+ segue em 110.
    let tight_panel = inner_w < 360;
    let logo_h: i32 = if tight_panel { 84 } else { 110 };
    let mut cy = if panel.has_logo {
        draw_image_absolute(canvas, images, PANEL_LOGO_IMG, x, y, inner_w, logo_h as u32);
        y + logo_h
    } else {
        draw_text_wrapped_absolute(
            canvas,
            font,
            x,
            y,
            inner_w,
            TextStyle::new(2.0, PANEL_TEXT),
            &panel.title,
        )
    };

    // 1b. O console (o bloco é o próprio PSX, `draw_panel_slot`): o disco
    // entra pela fenda na animação e fica escondido em repouso — arte ou
    // sem arte, o console está sempre na cena. Os slots de memory card do
    // corpo viram o hit da biblioteca de cards (plano §3).
    // O console está sempre na cena (o bloco 1b atribui incondicionalmente);
    // a linha abaixo silencia o clippy que não vê isso.
    #[allow(unused_assignments)]
    let mut console_face_hits: Option<FaceHits> = None;
    {
        cy += 4;
        // 316 px de console nas telas folgadas; nas apertadas o bloco é mais
        // alto — é onde cabem o slot de controle em duas linhas e o disco
        // maior (os 44 px extras vêm da logo mais baixa e do vão menor
        // acima da lista de comandos).
        let cartridge_h: u32 = if tight_panel { 360 } else { 316 };
        let (t, ejecting) = panel.cartridge_motion.unwrap_or((1.0, false));
        let face_hits = draw_panel_slot(
            canvas,
            images,
            Rect::new(x, cy, inner_w, cartridge_h),
            t,
            ejecting,
            panel.powered,
            panel.reset_pressed,
            spin,
            panel.lid_open,
            panel.card1_label.as_deref(),
            panel.card_motion,
            panel.analog1,
        );
        console_face_hits = Some(face_hits);
        cy += cartridge_h as i32;
    }

    // Below this y, stop — leave the session clock's own band (item 6)
    // clear. There are a lot more command rows than there used to be (plan
    // revision added six), so a long title plus a game with several curated
    // cheats can genuinely run out of room; a clean cutoff beats spilling
    // into the clock. No scrolling yet (a real gap, not silently accepted —
    // see `docs/fase-4.md`'s revision note).
    let limit = rect.bottom() - pad - (GLYPH_H as i32 + 12);

    // 2. Power / Eject / Reset vivem NA FACE do console (a foto do PS1 FAT:
    // Power redondo com o LED, Reset pequeno acima dele, Open/Eject do
    // outro lado no mesmo tamanho). Os hits chegam do bloco 1b.
    let mut buttons = Vec::new();
    if let Some(hits) = console_face_hits {
        buttons.push((PanelButton::Power, hits.power));
        buttons.push((PanelButton::Eject, hits.eject));
        buttons.push((PanelButton::Reset, hits.reset));
        buttons.push((PanelButton::Cards1, hits.cards1));
        // A label do card encaixado é um segundo hit da biblioteca (o
        // gesto "Trocar/Ejetar" impresso nela).
        if let Some(label) = hits.card_btn {
            buttons.push((PanelButton::Cards1, label));
        }
        buttons.push((PanelButton::Analog1, hits.analog1));
    }

    // 3. Commands — the console's own buttons, not the emulator's extras
    // (plan §3.2, item 3), every one of them clickable (plan revision:
    // mouse/gamepad only, no keyboard legend any more). Everything here
    // only acts while powered — Power/Eject/Reset are drawn separately
    // above (item 2) with their own dim rules, so they're filtered out of
    // this list rather than drawn twice.
    let other_commands: Vec<&(PanelButton, String, bool)> = panel
        .commands
        .iter()
        .filter(|(k, _, _)| {
            !matches!(
                k,
                PanelButton::Power | PanelButton::Eject | PanelButton::Reset
            )
        })
        .collect();
    if !other_commands.is_empty() && cy < limit {
        // Respiro entre o pé da face e a lista: o bay do memory card mora
        // na última faixa do console e a lista colada nele o espremia (plan
        // revision: "colocar o bloco de comandos mais pra baixo, pra deixar
        // o memory card mais visível") — o vão é a pista da label do card
        // (adesivo + "Trocar/Ejetar"), que desce além do pé da face.
        // Nas apertadas o vão encolhe para o mínimo da label do card (ela
        // desce 60 px além do pé da face) — a lista de comandos sobe junto
        // (plan revision: "suba um pouco mais o bloco de comandos").
        cy += if tight_panel { 26 } else { 80 };
        draw_text_absolute(
            canvas,
            font,
            x,
            cy,
            TextStyle::new(1.0, PANEL_DIM),
            "comandos",
            usize::MAX,
        );
        cy += GLYPH_H as i32 + 6;
    }
    for (kind, label, enabled) in other_commands {
        // Check the row's projected bottom, not just its top — otherwise
        // the last row drawn can still poke into the session clock's band.
        if cy + 4 + (GLYPH_H + 6) as i32 > limit {
            break;
        }
        cy += 4;
        // Tighter than a lone idle-screen button (plan revision: nine of
        // these stack now, not three — every extra pixel per row adds up).
        let btn = Rect::new(x, cy, inner_w, GLYPH_H + 6);
        // A row can ser desativada por conta própria (o Conquistas de jogo
        // sem set — plan revision: "mostre o botão mas mostre desativado"):
        // apagada mesmo com o console ligado. O hit continua valendo — o
        // caller conta o porquê no OSD.
        let drawn = draw_button(canvas, font, btn, label, panel.powered && *enabled);
        cy = drawn.bottom();
        buttons.push((*kind, drawn));
    }

    // 4. Cheats — just a count here (plan revision: used to list every
    // active description, one per line, which could run the panel out of
    // room on its own for a game with several curated cheats on at once;
    // the actual list lives in the Cheats modal, reachable from a
    // `commands` row above, so nothing is lost by not repeating it here).
    // Absent entirely with none on, not a "0 cheats" filler.
    let cheats_on_count = panel.cheats.iter().filter(|(_, on)| *on).count();
    // Room for the header plus one line — the count line never wraps to
    // more than that (it's always short: "N cheats ativados").
    let cheats_fit = cy + 16 + (GLYPH_H as i32 + 2) <= limit;
    if cheats_on_count > 0 && cheats_fit {
        cy += 16;
        let label = if cheats_on_count == 1 {
            "1 cheat ativado".to_string()
        } else {
            format!("{cheats_on_count} cheats ativados")
        };
        cy = draw_text_wrapped_absolute(
            canvas,
            font,
            x,
            cy,
            inner_w,
            TextStyle::new(1.0, PANEL_TEXT),
            &label,
        );
    }

    // 5. Notes — a pinned print's thumbnail + counter, and/or a pinned
    // text-note slot's content (plan §3.2, item 5; §3.4; plan revision:
    // the two are independent — either, both, or neither can be showing at
    // once, "mostrar apenas uma nota e/ou uma imagem"). Used to show
    // whatever was captured/viewed most recently regardless of pin, which
    // was surprising; now it's opt-in on both sides. Absent entirely with
    // nothing pinned either way, not a "no notes" filler. Same room-check
    // shape as cheats, above — the thumbnail and the text block each add
    // their own extra room when present.
    let text_h = panel
        .text_note
        .as_ref()
        .map_or(0, |t| wrapped_height(inner_w, 1, t) + 6);
    let notes_h = 16
        + (GLYPH_H as i32 + 6)
        + if panel.has_note_thumb { 70 + 6 } else { 0 }
        + if panel.note_count > 0 {
            GLYPH_H as i32 + 6
        } else {
            0
        }
        + text_h;
    if (panel.note_count > 0 || panel.text_note.is_some()) && cy + notes_h <= limit {
        cy += 16;
        draw_text_absolute(
            canvas,
            font,
            x,
            cy,
            TextStyle::new(1.0, PANEL_DIM),
            "notas",
            usize::MAX,
        );
        cy += GLYPH_H as i32 + 6;
        if panel.has_note_thumb {
            draw_image_absolute(canvas, images, PANEL_NOTE_IMG, x, cy, inner_w, 70);
            cy += 70 + 6;
        }
        if panel.note_count > 0 {
            let label = if panel.note_count == 1 {
                "1 slot fixado".to_string()
            } else {
                format!("{} slots fixados", panel.note_count)
            };
            draw_text_absolute(
                canvas,
                font,
                x,
                cy,
                TextStyle::new(1.0, PANEL_TEXT),
                &label,
                usize::MAX,
            );
            cy += GLYPH_H as i32 + 6;
        }
        if let Some(text) = &panel.text_note {
            draw_text_wrapped_absolute(
                canvas,
                font,
                x,
                cy,
                inner_w,
                TextStyle::new(1.0, PANEL_TEXT),
                text,
            );
        }
    }

    // 6. Session clock, pinned to the bottom (plan §3.2, item 6).
    let secs = session.as_secs();
    let stamp = if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
    } else {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    };
    let ty = rect.bottom() - pad - GLYPH_H as i32;
    draw_text_absolute(
        canvas,
        font,
        x,
        ty,
        TextStyle::new(1.0, PANEL_DIM),
        "sessão",
        usize::MAX,
    );
    let label_w = (GLYPH_W as i32) * "sessão ".chars().count() as i32;
    draw_text_absolute(
        canvas,
        font,
        x + label_w,
        ty,
        TextStyle::new(1.0, PANEL_TEXT),
        &stamp,
        usize::MAX,
    );

    buttons
}

/// Draw the shelf's own flat side panel (plan revision — moved out of the
/// warped screen buffer so its text and art render undistorted, the same
/// treatment the in-game panel already gets): logo/title, cartridge art,
/// play count, then whatever extra info the No-Intro DAT carried beyond the
/// bare title, and finally the shelf's own Voltar/Configuracoes buttons.
/// `None` (nothing selected yet — an empty catalogue) still draws the two
/// buttons. Returns the clickable buttons drawn this frame, in `rect`'s
/// (output/canvas) coordinate space — the caller stores them for
/// `Cabinet::hit_shelf_button`.
/// The settings screen's flat panel (plan revision: "coloque o painel ... se
/// for interessante faça secoes na configuração"): title, one button per
/// section (the current one lit), and "Voltar" pinned at the bottom — the
/// same bottom-button stack the shelf panel uses.
pub(super) fn draw_settings_panel(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    panel: Option<&SettingsPanelInfo>,
    rect: Rect,
) -> Vec<(SettingsButton, Rect)> {
    if rect.width() == 0 {
        return Vec::new();
    }
    canvas.set_draw_color(Color::RGB(PANEL_BG.0, PANEL_BG.1, PANEL_BG.2));
    let _ = canvas.fill_rect(rect);

    let pad = 20i32;
    let inner_w = rect.width().saturating_sub(pad as u32 * 2);
    let x = rect.x() + pad;
    let mut y = rect.y() + pad;

    let Some(panel) = panel else {
        return Vec::new();
    };
    draw_text_wrapped_absolute(
        canvas,
        font,
        x,
        y,
        inner_w,
        TextStyle::new(2.0, PANEL_TEXT),
        &panel.title,
    );
    y += 2 * GLYPH_H as i32 + 16;

    let btn_h = (GLYPH_H + 12) as i32;
    let back_rect = Rect::new(x, rect.bottom() - pad - btn_h, inner_w, btn_h as u32);
    let mut buttons = vec![(
        SettingsButton::Back,
        draw_button(canvas, font, back_rect, "Voltar", true),
    )];

    // Section buttons, top to bottom under the title — the currently shown
    // section lit, the others dim (same lit/dim language as every other
    // panel button).
    let limit = back_rect.y() - 12;
    for (i, name) in panel.sections.iter().enumerate() {
        if y + btn_h > limit {
            break;
        }
        let r = Rect::new(x, y, inner_w, btn_h as u32);
        buttons.push((
            SettingsButton::Section(i),
            draw_button(canvas, font, r, name, i == panel.selected),
        ));
        y += btn_h + 8;
    }

    buttons
}

pub(super) fn draw_shelf_panel(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    panel: Option<&ShelfPanelInfo>,
    rect: Rect,
) -> Vec<(ShelfButton, Rect)> {
    if rect.width() == 0 {
        return Vec::new();
    }
    canvas.set_draw_color(Color::RGB(PANEL_BG.0, PANEL_BG.1, PANEL_BG.2));
    let _ = canvas.fill_rect(rect);

    let pad = 20i32;
    let inner_w = rect.width().saturating_sub(pad as u32 * 2);
    let x = rect.x() + pad;
    let y = rect.y() + pad;
    let panel_favorite = panel.and_then(|p| p.favorite);
    let panel_achievements = panel.is_some_and(|p| p.achievements);
    let panel_ach_reason = panel.and_then(|p| p.achievements_reason.as_deref());

    // Stacked buttons at the bottom, "Favoritar" / "Conquistas" /
    // "Configurações" / "Voltar" top to bottom — "Voltar" always the last
    // one (plan revision: "botao voltar sempre o ultimo botao do painel,
    // acima dele coloque configurações e depois acima o de favoritos");
    // "Conquistas" sits between favoritar e configurações (plan revision:
    // "quando ligar o RA colocar botão de lista de conquistas entre
    // favoritar e configurações"); "Atualizar" moved out of the panel into
    // the header row beside "histórico".
    let btn_h = (GLYPH_H + 12) as i32;
    let back_rect = Rect::new(x, rect.bottom() - pad - btn_h, inner_w, btn_h as u32);
    let settings_rect = Rect::new(x, back_rect.y() - 8 - btn_h, inner_w, btn_h as u32);
    // A pilha cresce para cima: Conquistas (quando o RA identificou) e
    // Favoritar — cada um só quando existe, e o corte do corpo acompanha
    // o botão de cima de verdade. O Manual vive nas abas do topo.
    let mut top = settings_rect.y();
    let ach_rect = panel_achievements.then(|| {
        let r = Rect::new(x, top - 8 - btn_h, inner_w, btn_h as u32);
        top = r.y();
        r
    });
    let fav_rect = panel_favorite.map(|_| {
        let r = Rect::new(x, top - 8 - btn_h, inner_w, btn_h as u32);
        top = r.y();
        r
    });
    let mut buttons = vec![
        (
            ShelfButton::Back,
            draw_button(canvas, font, back_rect, "Voltar", true),
        ),
        (
            ShelfButton::Settings,
            draw_button(canvas, font, settings_rect, "Configurações", true),
        ),
    ];
    if let Some(ach_r) = ach_rect {
        // Bloqueado (RA marcou o hash como versão não suportada): botão
        // apagado com o motivo no rótulo e SEM hit — clicar não faz nada
        // (plan revision: "mostrar mas deixar desativado").
        match panel_ach_reason {
            Some(reason) => {
                let _ = draw_button(
                    canvas,
                    font,
                    ach_r,
                    &format!("Conquistas ({reason})"),
                    false,
                );
            }
            None => buttons.push((
                ShelfButton::ShelfAchievements,
                draw_button(canvas, font, ach_r, "Conquistas", true),
            )),
        }
    }
    // The favorite button sits above the trio, drawn before the early
    // `None` return so it never depends on the panel having content.
    if let (Some(fav), Some(fav_r)) = (panel_favorite, fav_rect) {
        buttons.push((
            ShelfButton::ToggleFavorite,
            draw_button(
                canvas,
                font,
                fav_r,
                if fav { "Remover favorito" } else { "Favoritar" },
                true,
            ),
        ));
    }

    let Some(panel) = panel else {
        return buttons;
    };

    // Everything above the buttons — same cutoff rule `draw_panel` uses
    // for its command rows: a clean stop beats spilling into the buttons.
    // The cutoff is the TOP OF THE FIRST BUTTON ACTUALLY DRAWN (the stack
    // grows upward from "Voltar") — an earlier revision stopped one button
    // short, which was harmless while that slot sat empty and became a real
    // overflow the moment the RA's "Conquistas" button filled it (the info
    // text ran straight over the button).
    let limit = top - 12;

    let cy = if let Some(id) = panel.logo_img {
        draw_image_absolute(canvas, images, id, x, y, inner_w, 110);
        y + 110
    } else {
        draw_text_wrapped_absolute(
            canvas,
            font,
            x,
            y,
            inner_w,
            TextStyle::new(2.0, PANEL_TEXT),
            &panel.title,
        )
    };

    // Abas (plan revision: "a ideia é mostrar um info de cada vez mesmo em
    // resolução grande, mudar ao clicar") — o corpo mostra SÓ os blocos da
    // seção ativa: capa traseira (imagem), cartucho (imagem) ou
    // informações (lançamento + campos do DAT/RA). Os três botões do topo
    // trocam a aba; a seção sem conteúdo mostra um aviso dim.
    let capa_blocks: Vec<PanelBlock> = panel
        .backcover_img
        .map(|id| vec![PanelBlock::Image(id, 280)])
        .unwrap_or_default();
    let cart_blocks: Vec<PanelBlock> = panel
        .cartridge_img
        .map(|id| vec![PanelBlock::Image(id, 210)])
        .unwrap_or_default();
    let mut info_blocks: Vec<PanelBlock> = Vec::new();
    if panel.release.is_some() || !panel.info.is_empty() {
        info_blocks.push(PanelBlock::Title("Informações Gerais"));
    }
    if let Some(release) = &panel.release {
        info_blocks.push(PanelBlock::Field("lançamento", release));
    }
    for (label, value) in &panel.info {
        // A linha de conquistas ganha a medalha de prêmio do RA quando o
        // servidor concedeu um.
        match panel.award_img.filter(|_| label == "conquistas") {
            Some(medal) => info_blocks.push(PanelBlock::RaField(label, value, medal)),
            None => info_blocks.push(PanelBlock::Field(label, value)),
        }
    }
    // O bloco HowLongToBeat fecha a aba: o cabeçalho com o ícone e uma
    // barra por estilo — em blocos separados para o scroll encaixar o que
    // couber (o bloco inteiro não cabia e o `break` o pulava inteiro).
    // A seção de nota da IGDB, entre as infos gerais e o HowLongToBeat.
    if let Some(igdb) = &panel.igdb {
        info_blocks.push(PanelBlock::SectionHead(igdb.icon, "IGDB"));
        info_blocks.push(PanelBlock::Field("nota", &igdb.nota));
    }
    if let Some(hltb) = &panel.hltb {
        if !hltb.rows.is_empty() {
            info_blocks.push(PanelBlock::SectionHead(hltb.icon, "HowLongToBeat"));
            for bar in &hltb.rows {
                info_blocks.push(PanelBlock::Field(&bar.label, &bar.text));
            }
        }
    }
    let manual_blocks: Vec<PanelBlock> = panel
        .manual_first_page
        .map(|id| vec![PanelBlock::Image(id, 280)])
        .unwrap_or_default();
    let (blocks, empty_label): (Vec<PanelBlock>, &str) = match panel.section {
        PanelSection::CapaTraseira => (capa_blocks, "sem capa traseira"),
        PanelSection::Cartucho => (cart_blocks, "sem disco"),
        PanelSection::Informacoes => (info_blocks, "sem informações"),
        PanelSection::Manual => (manual_blocks, "sem manual"),
    };

    let has = |s: PanelSection| match s {
        PanelSection::CapaTraseira => panel.backcover_img.is_some(),
        PanelSection::Cartucho => panel.cartridge_img.is_some(),
        PanelSection::Informacoes => {
            panel.release.is_some() || !panel.info.is_empty() || panel.hltb.is_some()
        }
        PanelSection::Manual => panel.manual_first_page.is_some(),
    };

    // A aba ativa transborda? (só "informações", com muitos campos,
    // realisticamente) — se sim, o `scroll` do caller pula blocos dentro
    // dela; as outras seções têm um bloco só.
    let total_h: i32 = blocks.iter().map(|b| panel_block_height(inner_w, b)).sum();
    let scrollable = cy + total_h > limit;

    let scroll_btn_h = btn_h;
    let start = if scrollable {
        panel.scroll.min(blocks.len().saturating_sub(1))
    } else {
        0
    };

    // Plan revision ("eu queria aqueles 3 botões de organização das infos
    // do game na tela grande também"): os saltos ficam SEMPRE visíveis com
    // jogo focused — em telas grandes o conteúdo costuma caber sem rolagem
    // e antes os botões nem apareciam. Sem transbordo o salto é no-op
    // (tudo já está na tela). Como a fileira de baixo das antigas setas
    // sumiu, o corpo ganha o espaço dela de volta.
    let gap = 6i32;
    let half = (inner_w as i32 - gap) / 2;
    let active = panel.section;
    // Quatro abas em duas fileiras: informações | capa traseira, depois
    // manual | disco (a aba "cartucho" do SNES virou "disco" — o PSX não
    // tem cartucho).
    let tabs = [
        (
            ShelfButton::PanelJump(PanelSection::Informacoes),
            "informações",
            active == PanelSection::Informacoes || has(PanelSection::Informacoes),
        ),
        (
            ShelfButton::PanelJump(PanelSection::CapaTraseira),
            "capa traseira",
            active == PanelSection::CapaTraseira || has(PanelSection::CapaTraseira),
        ),
        (
            ShelfButton::PanelJump(PanelSection::Manual),
            "manual",
            active == PanelSection::Manual || has(PanelSection::Manual),
        ),
        (
            ShelfButton::PanelJump(PanelSection::Cartucho),
            "disco",
            active == PanelSection::Cartucho || has(PanelSection::Cartucho),
        ),
    ];
    // Respiro entre a logo/título e a fileira de botões: sem ele os botões
    // nascem colados na borda de baixo da logo (plan revision: "baixar um
    // pouco para os botões não ficarem sobrepostos com a logo").
    let mut body_top = cy + 10;
    for (n, (btn, label, lit)) in tabs.iter().enumerate() {
        let row = (n / 2) as i32;
        let col = (n % 2) as i32;
        let r = Rect::new(
            x + col * (half + gap),
            body_top + row * (scroll_btn_h + 8),
            half as u32,
            scroll_btn_h as u32,
        );
        buttons.push((*btn, draw_button(canvas, font, r, label, *lit)));
    }
    body_top += (scroll_btn_h + 8) * 2;

    let mut cy = body_top;
    if blocks.is_empty() {
        // Aba escolhida sem conteúdo (não tem a arte local, o DAT não
        // trouxe campos) — aviso dim em vez de corpo vazio.
        let _ = draw_text_wrapped_absolute(
            canvas,
            font,
            x,
            cy,
            inner_w,
            TextStyle::new(1.0, PANEL_DIM),
            empty_label,
        );
    }
    for block in &blocks[start..] {
        let h = panel_block_height(inner_w, block);
        if cy + h > limit {
            break;
        }
        let block_top = cy;
        cy = draw_panel_block(canvas, font, images, x, cy, inner_w, block);
        // The back cover is clickable (plan revision: "ao clicar no back
        // cover possibilitar mostrar em tamanho maior, com botão de
        // fechar") — outline it as the affordance and hand the hit rect up.
        if let (PanelBlock::Image(id, _), Some(bc)) = (block, panel.backcover_img) {
            if *id == bc {
                let hit = Rect::new(x, block_top, inner_w, h.max(1) as u32);
                // Borda preta (plan revision: "a borda do back cover coloque
                // em preto") — discreta sobre o painel claro.
                canvas.set_draw_color(Color::RGBA(0, 0, 0, 200));
                let _ = canvas.draw_rect(hit);
                buttons.push((ShelfButton::Backcover, hit));
            }
        }
        // O cartucho ganha o mesmo tratamento do back cover (plan revision:
        // "colocar zoom igual ao backcover no cartucho").
        if let (PanelBlock::Image(id, _), Some(cc)) = (block, panel.cartridge_img) {
            if *id == cc {
                let hit = Rect::new(x, block_top, inner_w, h.max(1) as u32);
                canvas.set_draw_color(Color::RGBA(0, 0, 0, 200));
                let _ = canvas.draw_rect(hit);
                buttons.push((ShelfButton::Cartridge, hit));
            }
        }
        // E a primeira página do manual (plan revision: "ao clicar abrir
        // nele visualização") — abre o leitor de páginas.
        if let (PanelBlock::Image(id, _), Some(mp)) = (block, panel.manual_first_page) {
            if *id == mp {
                let hit = Rect::new(x, block_top, inner_w, h.max(1) as u32);
                canvas.set_draw_color(Color::RGBA(0, 0, 0, 200));
                let _ = canvas.draw_rect(hit);
                buttons.push((ShelfButton::Manual, hit));
            }
        }
    }

    // (sem fileira de setas embaixo — os saltos nomeados acima cobrem a
    // navegação do corpo)

    buttons
}

/// One scrollable piece of the shelf panel's body (plan revision: "criar
/// rolagem no painel quando necessario") — see `draw_shelf_panel`.
/// `Release` stays its own variant rather than folding into `Field`
/// because it draws label and value on the *same* line (a small
/// headline stat), unlike `Field`'s stacked label-then-value.
pub(super) enum PanelBlock<'a> {
    Image(u64, u32),
    /// Título de seção da aba ("Informações Gerais"), centralizado.
    Title(&'a str),
    /// label/valor com a medalha de prêmio à esquerda do valor — a linha
    /// "conquistas" quando o servidor concedeu um prêmio ao jogo.
    RaField(&'a str, &'a str, u64),
    Field(&'a str, &'a str),
    /// Cabeçalho de seção: ícone + título em negrito (IGDB, HowLongToBeat).
    SectionHead(u64, &'a str),
}

/// `block`'s height in pixels, gap included — must match `draw_panel_block`
/// exactly, or "does it fit" and "how tall did it draw" disagree (the bug
/// `draw_shelf_panel`'s own history note above the `info` loop describes).
pub(super) fn panel_block_height(inner_w: u32, block: &PanelBlock) -> i32 {
    match block {
        PanelBlock::Image(_, h) => 8 + *h as i32,
        PanelBlock::Title(_) => 10 + GLYPH_H as i32,
        PanelBlock::RaField(label, value, _) => {
            10 + wrapped_height(inner_w, 1, label) + wrapped_height(inner_w, 1, value)
        }
        PanelBlock::Field(label, value) => {
            10 + wrapped_height(inner_w, 1, label) + wrapped_height(inner_w, 1, value)
        }
        PanelBlock::SectionHead(_, _) => 8 + HLTB_ICON + 4,
    }
}

/// Ícone do HowLongToBeat no cabeçalho do bloco (quadrado, com o glifo da
/// legenda dentro) e altura da barra de tempo (o glifo + respiro em cima e
/// embaixo para o tempo caber escrito dentro).
pub(super) const HLTB_ICON: i32 = 40;

/// Draw `block` at `cy`, returning the y just past it.
pub(super) fn draw_panel_block(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    x: i32,
    cy: i32,
    inner_w: u32,
    block: &PanelBlock,
) -> i32 {
    match block {
        PanelBlock::Image(id, h) => {
            let cy = cy + 8;
            draw_image_absolute(canvas, images, *id, x, cy, inner_w, *h);
            cy + *h as i32
        }
        PanelBlock::Title(t) => {
            let cy = cy + 10;
            draw_text_centered_absolute(
                canvas,
                font,
                x,
                cy,
                inner_w,
                TextStyle::bold(1.0, PANEL_TEXT),
                t,
            )
        }
        PanelBlock::RaField(label, value, medal) => {
            let cy = cy + 10;
            let cy = draw_text_centered_absolute(
                canvas,
                font,
                x,
                cy,
                inner_w,
                TextStyle::new(1.0, PANEL_DIM),
                label,
            );
            // Número + medalha como uma unidade, centrados — a fonte é
            // monoespaçada (GLYPH_W por caractere no size 1), então o fim do
            // texto é conhecido sem medição.
            let value_w = value.chars().count() as i32 * GLYPH_W as i32;
            let total = value_w + AWARD_MEDAL_GAP + AWARD_MEDAL_W;
            let vx = if total <= inner_w as i32 {
                x + (inner_w as i32 - total) / 2
            } else {
                x
            };
            draw_text_absolute(
                canvas,
                font,
                vx,
                cy,
                TextStyle::new(1.0, PANEL_TEXT),
                value,
                usize::MAX,
            );
            draw_image_absolute(
                canvas,
                images,
                *medal,
                vx + value_w + AWARD_MEDAL_GAP,
                cy + (GLYPH_H as i32 - AWARD_MEDAL_H) / 2,
                AWARD_MEDAL_W as u32,
                AWARD_MEDAL_H as u32,
            );
            cy + GLYPH_H as i32
        }
        PanelBlock::Field(label, value) => {
            let cy = cy + 10;
            let cy = draw_text_centered_absolute(
                canvas,
                font,
                x,
                cy,
                inner_w,
                TextStyle::new(1.0, PANEL_DIM),
                label,
            );
            draw_text_centered_absolute(
                canvas,
                font,
                x,
                cy,
                inner_w,
                TextStyle::new(1.0, PANEL_TEXT),
                value,
            )
        }
        PanelBlock::SectionHead(icon, titulo) => {
            // Cabeçalho de seção centrado: o ícone + o título em negrito
            // como uma unidade.
            let cy = cy + 8;
            let legenda = *titulo;
            let total = HLTB_ICON + 6 + legenda.len() as i32 * (GLYPH_W as i32 + 1) - 1;
            let hx = x + (inner_w as i32 - total) / 2;
            draw_image_absolute(
                canvas,
                images,
                *icon,
                hx,
                cy,
                HLTB_ICON as u32,
                HLTB_ICON as u32,
            );
            draw_text_absolute(
                canvas,
                font,
                hx + HLTB_ICON + 8,
                cy + (HLTB_ICON - GLYPH_H as i32) / 2,
                TextStyle::bold(1.0, PANEL_TEXT),
                legenda,
                usize::MAX,
            );
            cy + HLTB_ICON + 4
        }
    }
}

/// Draw one clickable panel button: a filled box (brighter/bordered when
/// `lit`, flush with the panel background otherwise) with `text` centered
/// inside — clipped to fit the box, with a trailing `...` if it doesn't
/// (plan revision: `text` used to just draw past the box's edge unclipped
/// when too long for it, visually merging into whatever sat next to it —
/// harmless while every label here was short, a real problem once the
/// Cheats modal started passing database-length descriptions through).
/// Returns the box's rect for hit-testing.
pub(super) fn draw_button(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    rect: Rect,
    text: &str,
    lit: bool,
) -> Rect {
    let (bg, border, fg) = if lit {
        (PANEL_BTN_BG, PANEL_TEXT, PANEL_TEXT)
    } else {
        (PANEL_BG, PANEL_BTN_BG, PANEL_DIM)
    };
    canvas.set_draw_color(Color::RGB(border.0, border.1, border.2));
    let _ = canvas.fill_rect(rect);
    let inset = Rect::new(
        rect.x() + 1,
        rect.y() + 1,
        rect.width() - 2,
        rect.height() - 2,
    );
    canvas.set_draw_color(Color::RGB(bg.0, bg.1, bg.2));
    let _ = canvas.fill_rect(inset);

    let side_pad = 6i32;
    let max_chars = (((rect.width() as i32 - side_pad * 2) / GLYPH_W as i32).max(1)) as usize;
    let shown = clip_label(text, max_chars);
    let text_w = (GLYPH_W as i32) * shown.chars().count() as i32;
    let tx = rect.x() + (rect.width() as i32 - text_w).max(4) / 2;
    let ty = rect.y() + (rect.height() as i32 - GLYPH_H as i32) / 2;
    draw_text_absolute(
        canvas,
        font,
        tx,
        ty,
        TextStyle::new(1.0, fg),
        &shown,
        usize::MAX,
    );
    rect
}

/// Uma linha de modal com ícone (memory cards): o mesmo corpo do
/// `draw_button`, com o quadrado do ícone 16×16 (ampliado para caber) à
/// esquerda e o rótulo ao lado dele, alinhado à esquerda.
pub(super) fn draw_icon_row(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    rect: Rect,
    icon: &Texture,
    text: &str,
    lit: bool,
) -> Rect {
    let (bg, border, fg) = if lit {
        (PANEL_BTN_BG, PANEL_TEXT, PANEL_TEXT)
    } else {
        (PANEL_BG, PANEL_BTN_BG, PANEL_DIM)
    };
    canvas.set_draw_color(Color::RGB(border.0, border.1, border.2));
    let _ = canvas.fill_rect(rect);
    canvas.set_draw_color(Color::RGB(bg.0, bg.1, bg.2));
    let _ = canvas.fill_rect(Rect::new(
        rect.x() + 1,
        rect.y() + 1,
        rect.width() - 2,
        rect.height() - 2,
    ));

    // O ícone: quadrado centralizado na altura, ampliado de 16×16 com o
    // Nearest ligado (pixels retos, nada de borrao).
    let side = (rect.height() as i32 - 8).max(8);
    let icon_rect = Rect::new(
        rect.x() + 4,
        rect.y() + (rect.height() as i32 - side) / 2,
        side as u32,
        side as u32,
    );
    let _ = canvas.copy(icon, None, icon_rect);

    let label_x = icon_rect.right() + 6;
    let max_chars = ((rect.right() - 6 - label_x).max(0) / GLYPH_W as i32).max(1) as usize;
    let shown = clip_label(text, max_chars);
    draw_text_absolute(
        canvas,
        font,
        label_x,
        rect.y() + (rect.height() as i32 - GLYPH_H as i32) / 2,
        TextStyle::new(1.0, fg),
        &shown,
        usize::MAX,
    );
    rect
}
