//! Gera o "Manual do console" - o PDF que o botão de mesmo nome da tela
//! inicial abre no tubo. O leitor do app só mostra páginas com imagem
//! embutida (o pdfium não vai no pacote), então cada página é desenhada aqui
//! como bitmap (fonte bitmap do próprio app, papel creme, capturas de tela
//! da pasta `packaging/manual-console/shots/`) e o PDF sai como uma imagem
//! JPEG por página.
//!
//! Rodar da raiz do repositório:
//! `cargo run -p polystnx-app --release --example gen_console_manual`
//! Saída: `crates/app/assets/console-manual-pt.pdf` (embutido no binário).
//! As capturas se refazem com os exemplos `fish_shot`, `tv_shot`,
//! `generic_cd_shot` e `polystnx --debug-idle-shot / --debug-settings`.

use std::path::{Path, PathBuf};

use image::imageops::{self, FilterType};
use image::{codecs::jpeg::JpegEncoder, Rgb, RgbImage};
use noto_sans_mono_bitmap::{get_raster, get_raster_width, FontWeight, RasterHeight};

const W: u32 = 1000;
const H: u32 = 1450;
const MARGIN: i32 = 70;

// Paleta do papel e da tinta.
const PAPEL: Rgb<u8> = Rgb([242, 237, 227]);
const TINTA: Rgb<u8> = Rgb([54, 48, 42]);
const CORPO: Rgb<u8> = Rgb([74, 67, 58]);
const CINZA: Rgb<u8> = Rgb([148, 141, 130]);
const VERMELHO: Rgb<u8> = Rgb([200, 40, 48]);
const PRETO: Rgb<u8> = Rgb([26, 21, 18]);
const FILETE: Rgb<u8> = Rgb([214, 207, 193]);

#[derive(Clone, Copy)]
enum Fonte {
    Corpo,
    Negrito,
    Legenda,
    Titulo,
    Capa,
}

impl Fonte {
    fn par(self) -> (FontWeight, RasterHeight) {
        match self {
            Fonte::Corpo => (FontWeight::Regular, RasterHeight::Size20),
            Fonte::Negrito => (FontWeight::Bold, RasterHeight::Size20),
            Fonte::Legenda => (FontWeight::Regular, RasterHeight::Size16),
            Fonte::Titulo | Fonte::Capa => (FontWeight::Bold, RasterHeight::Size32),
        }
    }

    fn celula(self) -> i32 {
        let (w, h) = self.par();
        get_raster_width(w, h) as i32
    }

    fn altura(self) -> i32 {
        self.par().1.val() as i32
    }
}

/// Escreve `s` em `(x, y)`; `escala` amplia a célula (pixels grossos para a
/// capa). Mistura o alfa do glifo com o papel.
fn texto(img: &mut RgbImage, x: i32, y: i32, f: Fonte, escala: i32, cor: Rgb<u8>, s: &str) {
    let (peso, alt) = f.par();
    let mut pen = x;
    for ch in s.chars() {
        if let Some(g) = get_raster(ch, peso, alt) {
            for (gy, linha) in g.raster().iter().enumerate() {
                for (gx, &a) in linha.iter().enumerate() {
                    if a == 0 {
                        continue;
                    }
                    for dy in 0..escala {
                        for dx in 0..escala {
                            let px = pen + gx as i32 * escala + dx;
                            let py = y + gy as i32 * escala + dy;
                            if px < 0 || py < 0 || px >= W as i32 || py >= H as i32 {
                                continue;
                            }
                            let dst = img.get_pixel_mut(px as u32, py as u32);
                            let k = a as f32 / 255.0;
                            for c in 0..3 {
                                dst[c] = (cor[c] as f32 * k + dst[c] as f32 * (1.0 - k)) as u8;
                            }
                        }
                    }
                }
            }
        }
        pen += f.celula() * escala;
    }
}

/// Quebra `s` em linhas de até `cols` colunas (por palavra).
fn quebrar(s: &str, cols: usize) -> Vec<String> {
    let mut out = Vec::new();
    for par in s.split('\n') {
        let mut linha = String::new();
        for palavra in par.split_whitespace() {
            let cabe =
                linha.is_empty() || linha.chars().count() + 1 + palavra.chars().count() <= cols;
            if !cabe {
                out.push(std::mem::take(&mut linha));
            }
            if !linha.is_empty() {
                linha.push(' ');
            }
            linha.push_str(palavra);
        }
        out.push(linha);
    }
    out
}

/// Uma página sendo montada: o papel e o cursor vertical.
struct Pagina {
    img: RgbImage,
    y: i32,
    numero: usize,
}

impl Pagina {
    fn nova(numero: usize, titulo: Option<&str>) -> Self {
        let mut img = RgbImage::from_pixel(W, H, PAPEL);
        // Folha: o filete do topo e o número de página embaixo.
        for x in MARGIN..(W as i32 - MARGIN) {
            img.put_pixel(x as u32, 62, FILETE);
        }
        texto(
            &mut img,
            MARGIN,
            34,
            Fonte::Legenda,
            1,
            CINZA,
            "PolystnX - Manual do console",
        );
        let rodape = format!("{numero}");
        texto(
            &mut img,
            W as i32 / 2 - Fonte::Corpo.celula() * rodape.len() as i32 / 2,
            H as i32 - 58,
            Fonte::Corpo,
            1,
            CINZA,
            &rodape,
        );
        let mut p = Pagina {
            img,
            y: 110,
            numero,
        };
        if let Some(t) = titulo {
            p.titulo(t);
        }
        p
    }

    fn cols(&self, f: Fonte) -> usize {
        ((W as i32 - MARGIN * 2) / f.celula()).max(10) as usize
    }

    fn titulo(&mut self, t: &str) {
        texto(&mut self.img, MARGIN, self.y, Fonte::Titulo, 1, TINTA, t);
        self.y += Fonte::Titulo.altura() + 8;
        for x in MARGIN..(MARGIN + 120) {
            for dy in 0..4 {
                self.img.put_pixel(x as u32, (self.y + dy) as u32, VERMELHO);
            }
        }
        self.y += 34;
    }

    fn subtitulo(&mut self, t: &str) {
        self.y += 10;
        texto(
            &mut self.img,
            MARGIN,
            self.y,
            Fonte::Negrito,
            1,
            VERMELHO,
            t,
        );
        self.y += Fonte::Negrito.altura() + 10;
    }

    fn paragrafo(&mut self, t: &str) {
        for linha in quebrar(t, self.cols(Fonte::Corpo)) {
            texto(
                &mut self.img,
                MARGIN,
                self.y,
                Fonte::Corpo,
                1,
                CORPO,
                &linha,
            );
            self.y += Fonte::Corpo.altura() + 8;
        }
        self.y += 10;
    }

    fn itens(&mut self, itens: &[&str]) {
        let cols = self.cols(Fonte::Corpo) - 4;
        for it in itens {
            let linhas = quebrar(it, cols);
            for (i, linha) in linhas.iter().enumerate() {
                if i == 0 {
                    texto(
                        &mut self.img,
                        MARGIN + 8,
                        self.y,
                        Fonte::Negrito,
                        1,
                        VERMELHO,
                        "-",
                    );
                }
                texto(
                    &mut self.img,
                    MARGIN + 8 + Fonte::Corpo.celula() * 2,
                    self.y,
                    Fonte::Corpo,
                    1,
                    CORPO,
                    linha,
                );
                self.y += Fonte::Corpo.altura() + 8;
            }
            self.y += 4;
        }
        self.y += 8;
    }

    /// Uma captura de tela com legenda, centralizada na largura útil.
    fn figura(&mut self, arquivo: &Path, legenda: &str, altura_max: u32) {
        let shot = recortar_bordas(
            image::open(arquivo)
                .unwrap_or_else(|e| {
                    panic!("captura {}: {e}", arquivo.display());
                })
                .to_rgb8(),
        );
        let largura_max = W - MARGIN as u32 * 2;
        let k = (largura_max as f32 / shot.width() as f32)
            .min(altura_max as f32 / shot.height() as f32);
        let (nw, nh) = (
            (shot.width() as f32 * k) as u32,
            (shot.height() as f32 * k) as u32,
        );
        let img = imageops::resize(&shot, nw, nh, FilterType::Lanczos3);
        let x = (W - nw) as i32 / 2;
        // Moldura: uma borda escura de 3 px, como uma foto impressa.
        for dy in -3..(nh as i32 + 3) {
            for dx in -3..(nw as i32 + 3) {
                let (px, py) = (x + dx, self.y + dy);
                if px >= 0 && py >= 0 && px < W as i32 && py < H as i32 {
                    let borda = dx < 0 || dy < 0 || dx >= nw as i32 || dy >= nh as i32;
                    if borda {
                        self.img.put_pixel(px as u32, py as u32, PRETO);
                    }
                }
            }
        }
        imageops::overlay(&mut self.img, &img, x as i64, self.y as i64);
        self.y += nh as i32 + 14;
        let cols = self.cols(Fonte::Legenda);
        for linha in quebrar(legenda, cols) {
            let w = linha.chars().count() as i32 * Fonte::Legenda.celula();
            texto(
                &mut self.img,
                (W as i32 - w) / 2,
                self.y,
                Fonte::Legenda,
                1,
                CINZA,
                &linha,
            );
            self.y += Fonte::Legenda.altura() + 4;
        }
        self.y += 18;
    }

    fn restante(&self) -> i32 {
        H as i32 - 100 - self.y
    }
}

/// Tira as faixas pretas das bordas da captura (o tubo vem rodeado de
/// preto quando a janela da captura é mais larga que o gabinete).
fn recortar_bordas(img: RgbImage) -> RgbImage {
    let (w, h) = img.dimensions();
    let escuro = |x: u32, y: u32| {
        let p = img.get_pixel(x, y);
        p[0] < 6 && p[1] < 6 && p[2] < 6
    };
    let col_preta = |x: u32| (0..h).step_by(4).all(|y| escuro(x, y));
    let (mut x0, mut x1) = (0, w);
    while x0 < w / 3 && col_preta(x0) {
        x0 += 1;
    }
    while x1 > w * 2 / 3 && col_preta(x1 - 1) {
        x1 -= 1;
    }
    imageops::crop_imm(&img, x0, 0, x1 - x0, h).to_image()
}

fn jpeg(img: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 86)
        .encode_image(img)
        .expect("jpeg");
    out
}

/// Um PDF mínimo: uma imagem JPEG de página inteira por página.
fn escrever_pdf(paginas: &[Vec<u8>]) -> Vec<u8> {
    let (pw, ph) = (500, 725);
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    let n = paginas.len();
    let mut obj = |out: &mut Vec<u8>, corpo: &[u8]| {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        out.extend_from_slice(corpo);
        out.extend_from_slice(b"\nendobj\n");
    };
    obj(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: String = (0..n).map(|i| format!("{} 0 R ", 3 + i * 3)).collect();
    obj(
        &mut out,
        format!("<< /Type /Pages /Kids [{kids}] /Count {n} >>").as_bytes(),
    );
    for (i, jpg) in paginas.iter().enumerate() {
        let base = 3 + i * 3;
        obj(
            &mut out,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw} {ph}] /Resources << /XObject << /Im0 {} 0 R >> >> /Contents {} 0 R >>",
                base + 2,
                base + 1
            )
            .as_bytes(),
        );
        let conteudo = format!("q {pw} 0 0 {ph} 0 0 cm /Im0 Do Q");
        obj(
            &mut out,
            format!(
                "<< /Length {} >>\nstream\n{conteudo}\nendstream",
                conteudo.len()
            )
            .as_bytes(),
        );
        let mut img = format!(
            "<< /Type /XObject /Subtype /Image /Width {W} /Height {H} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
            jpg.len()
        )
        .into_bytes();
        img.extend_from_slice(jpg);
        img.extend_from_slice(b"\nendstream");
        obj(&mut out, &img);
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes(),
    );
    for o in &offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn capa(logo: &Path) -> RgbImage {
    let mut img = RgbImage::from_pixel(W, H, PRETO);
    let logo = image::open(logo).expect("logo").to_rgba8();
    let k = 560.0 / logo.width() as f32;
    let (lw, lh) = (
        (logo.width() as f32 * k) as u32,
        (logo.height() as f32 * k) as u32,
    );
    let logo = imageops::resize(&logo, lw, lh, FilterType::Lanczos3);
    // A logo sobre o preto (alfa).
    let (ox, oy) = ((W - lw) as i64 / 2, 180);
    for (x, y, p) in logo.enumerate_pixels() {
        let a = p[3] as f32 / 255.0;
        let dst = img.get_pixel_mut((ox + x as i64) as u32, (oy + y as i64) as u32);
        for c in 0..3 {
            dst[c] = (p[c] as f32 * a + dst[c] as f32 * (1.0 - a)) as u8;
        }
    }
    let t = "MANUAL DO CONSOLE";
    let esc = 2;
    texto(
        &mut img,
        (W as i32 - Fonte::Capa.celula() * esc * t.len() as i32) / 2,
        oy as i32 + lh as i32 + 80,
        Fonte::Capa,
        esc,
        PAPEL,
        t,
    );
    let versao = format!("versão {}", env!("CARGO_PKG_VERSION"));
    texto(
        &mut img,
        (W as i32 - Fonte::Corpo.celula() * versao.len() as i32) / 2,
        oy as i32 + lh as i32 + 170,
        Fonte::Corpo,
        1,
        CINZA,
        &versao,
    );
    // O filete vermelho da marca.
    for x in (W as i32 / 2 - 60)..(W as i32 / 2 + 60) {
        for dy in 0..5 {
            img.put_pixel(x as u32, (oy as i32 + lh as i32 + 50 + dy) as u32, VERMELHO);
        }
    }
    img
}

fn main() -> anyhow::Result<()> {
    let raiz = PathBuf::from(".");
    let shots = raiz.join("packaging/manual-console/shots");
    let logo = raiz.join("crates/app/assets/console_logo.png");
    let saida = raiz.join("crates/app/assets/console-manual-pt.pdf");
    let shot = |n: &str| shots.join(n);
    let mut paginas: Vec<RgbImage> = vec![capa(&logo)];

    // 2 - Bem-vindo e índice.
    let mut p = Pagina::nova(2, Some("Bem-vindo"));
    p.paragrafo(
        "O PolystnX é um console de PlayStation inteiro dentro do seu computador: uma TV de tubo, um console com tampa e botões, uma estante de discos e o acessório de cheats de antigamente. Este manual mostra como usar cada parte.",
    );
    p.subtitulo("Neste manual");
    p.itens(&[
        "Primeiros passos - as pastas, o núcleo e a BIOS",
        "A estante - seus discos, a arte e o CD genérico",
        "Ligando o console - tampa, reset, memory cards e pausa",
        "A TV - POWER, volume e o menu de imagem",
        "Cheater Fish - escolha os cheats antes do jogo",
        "Controles",
        "Configurações",
        "Conquistas e memory cards",
        "Quando algo não funciona",
    ]);
    p.figura(
        &shot("inicio.png"),
        "A tela inicial: a TV no \"AV 1\", a estante e o manual do console no painel.",
        520,
    );
    paginas.push(p.img);

    // 3 - Primeiros passos.
    let mut p = Pagina::nova(3, Some("Primeiros passos"));
    p.paragrafo(
        "O app guarda tudo numa pasta própria: no macOS, ~/Library/Application Support/PolystnX; no Linux, ~/.local/share/PolystnX; no Windows, a pasta do próprio programa. Ela nasce sozinha na primeira abertura, com estas subpastas:",
    );
    p.itens(&[
        "bios/ - a BIOS do console (SCPH*.BIN). O app nunca a baixa: você coloca o arquivo aqui.",
        "roms/ - seus discos: .chd, ou .m3u para jogos de vários discos.",
        "core/ - o núcleo PCSX Rearmed, que emula o PlayStation.",
        "memcards/ - os memory cards (.mcr), um arquivo por card.",
        "saves/ e notes/ - estados salvos, tempo de jogo, notas e prints por jogo.",
        "assets/ - capas, discos, contracapas, logos e manuais em PDF (opcional).",
    ]);
    p.subtitulo("Em quatro passos");
    p.itens(&[
        "Na tela inicial, clique em \"Baixar núcleo\". O app confere o arquivo antes de instalar.",
        "Coloque sua BIOS em bios/. Em Configurações > bios você escolhe qual usar.",
        "Coloque seus discos em roms/.",
        "Clique em \"Estante de games\" e escolha um jogo.",
    ]);
    p.paragrafo(
        "O núcleo e a BIOS não vêm no pacote: o núcleo é de outro projeto (GPL) e a BIOS é propriedade da Sony. Use discos e BIOS que você tem direito de usar.",
    );
    paginas.push(p.img);

    // 4 - A estante.
    let mut p = Pagina::nova(4, Some("A estante"));
    p.paragrafo(
        "Cada disco de roms/ vira uma caixa na estante. O app identifica o jogo pelo serial do disco e mostra o nome, o ano e a editora que conhece; instalado o DAT No-Intro (Configurações > sistema), usa o nome oficial.",
    );
    p.itens(&[
        "Clique numa caixa (ou use o d-pad e A) para escolher; o botão de play aparece quando o cursor passa por cima.",
        "O painel ao lado mostra as informações, as conquistas e a nota do jogo, e deixa marcar favoritos. Os favoritos e os jogos recentes ganham uma faixa acima da grade.",
        "\"Atualizar\" relê a pasta roms/. \"Histórico\" lista os jogos mais jogados.",
    ]);
    p.subtitulo("Arte e CD genérico");
    p.paragrafo(
        "Se você colocar arte em assets/ (cover, disc, backcover, logo, com o mesmo nome do jogo), ela aparece na estante e no painel. Sem arte, o jogo ganha um CD-R de prata numa caixa de CD transparente, com o nome e o ano escritos à caneta - sempre do mesmo jeito para o mesmo jogo.",
    );
    p.figura(
        &shot("estante-cd.png"),
        "Jogos sem arte: o CD gravado, com o nome à caneta.",
        560,
    );
    paginas.push(p.img);

    // 5 - Ligando o console.
    let mut p = Pagina::nova(5, Some("Ligando o console"));
    p.paragrafo(
        "Ao escolher um jogo, o disco entra no console. Como no aparelho de verdade, ele só liga quando você aperta o botão POWER do painel. Daí em diante:",
    );
    p.itens(&[
        "POWER liga e desliga o console. Desligado, o jogo fica onde estava; ligando de novo, ele continua.",
        "OPEN/CLOSE abre a tampa: o jogo vê a bandeja abrir. Com o console desligado, serve para ejetar o disco e voltar à estante.",
        "RESET reinicia o jogo.",
        "O slot de memory card abre a biblioteca de cards: crie, encaixe, renomeie ou apague. Os saves do jogo ficam no card encaixado.",
    ]);
    p.subtitulo("A pausa");
    p.paragrafo(
        "O livro de pausa abre por Select + Start no controle (ou pelo botão da tela). Nele você salva e carrega estados, tira prints, escreve notas e vê as conquistas. Cada jogo tem as suas 15 páginas de notas e prints; páginas fixadas aparecem no painel.",
    );
    p.paragrafo("Se você minimizar a janela durante o jogo, ele pausa sozinho e espera a volta.");
    paginas.push(p.img);

    // 6 - A TV.
    let mut p = Pagina::nova(6, Some("A TV"));
    p.paragrafo(
        "O jogo aparece numa TV de tubo, com a marca PolyStnX embaixo e os botões físicos no queixo: VOL -, VOL +, MENU e POWER. Eles funcionam em qualquer tela, com o mouse ou, na tela inicial, com o controle.",
    );
    p.itens(&[
        "POWER liga e desliga o tubo. Ao desligar, a imagem é esmagada numa linha clara, a linha vira um ponto e o ponto some devagar, como nas TVs de tubo do fim dos anos 90. Ao ligar, o tubo fica escuro por um instante e a imagem sobe do preto. Desligada, a TV não mostra nem toca nada, mas o jogo segue rodando.",
        "VOL - e VOL + ajustam o volume (0 a 16), com as barrinhas verdes na tela.",
        "MENU abre o menu de imagem: BRILHO, CONTRASTE, COR e NITIDEZ. Cada toque em MENU passa para o próximo item; VOL - e VOL + ajustam; depois do último ele fecha.",
    ]);
    p.paragrafo(
        "O volume e a imagem ficam guardados para a próxima vez. Em Configurações > imagem e som você ajusta os mesmos valores. O 50 é o ponto neutro da imagem.",
    );
    p.figura(
        &shot("tv-menu.png"),
        "O menu de imagem e o volume na TV, com os botões do queixo.",
        560,
    );
    paginas.push(p.img);

    // 7 - Cheater Fish.
    let mut p = Pagina::nova(7, Some("Cheater Fish"));
    p.paragrafo(
        "O Cheater Fish é o acessório de cheats: um peixe de óculos que liga antes do jogo, como os cartuchos de trapaça dos anos 90. Ao ligar o console pela primeira vez com um disco que tem cheats, aparece o carregamento e, direto, a lista dos cheats daquele jogo.",
    );
    p.figura(
        &shot("peixe-carregando.png"),
        "O carregamento do Cheater Fish.",
        380,
    );
    p.itens(&[
        "Cima/baixo rolam a lista; A (ou o clique) liga e desliga o cheat marcado - a caixinha fica verde.",
        "\"Start Game\" (ou B) começa o jogo com os cheats que ficaram ligados.",
        "As escolhas ficam guardadas por jogo. Para mudar os cheats, desligue o console e ligue de novo: o Cheater Fish aparece outra vez.",
    ]);
    p.figura(&shot("peixe-lista.png"), "A lista de cheats do jogo.", 380);
    p.paragrafo(
        "O Cheater Fish só aparece no primeiro ligar da sessão e nunca com o RetroAchievements em modo hardcore. Dá para desligá-lo em Configurações > jogo.",
    );
    paginas.push(p.img);

    // 8 - Controles.
    let mut p = Pagina::nova(8, Some("Controles"));
    p.paragrafo(
        "O app foi pensado para jogar só com o controle, inclusive no Steam Deck. O controle é o DualShock: os botões do seu gamepad viram os do PlayStation.",
    );
    p.itens(&[
        "Menus e estante: o d-pad anda, A confirma, B volta.",
        "Tela inicial: o d-pad passeia pelos botões do painel, pelas setas de atualização e pelos botões da TV, com um anel lavanda no foco.",
        "Dentro do jogo: o jogo é dono de todos os botões. Select + Start abre e fecha a pausa.",
        "LB/RB trocam de seção nas Configurações.",
        "Teclado e mouse também funcionam; os atalhos de teclado do jogo se mudam em Configurações > controles.",
    ]);
    p.paragrafo(
        "Em Configurações > controles, escolha a ação e aperte a tecla ou o botão novo para remapear. Trocar um botão que já era de outra ação troca os dois.",
    );
    p.paragrafo(
        "O botão ANALOG do console liga o modo analógico e a vibração de uma vez, como o do DualShock: o LED vermelho vira verde.",
    );
    paginas.push(p.img);

    // 9 - Configurações.
    let mut p = Pagina::nova(9, Some("Configurações"));
    p.paragrafo(
        "As configurações têm sete seções, no painel à direita; o clique troca de seção, ou LB/RB no controle. Tudo é gravado na hora, não há botão de aplicar.",
    );
    p.itens(&[
        "controles - remapear teclado e botões do controle.",
        "jogo - liga e desliga o Cheater Fish.",
        "imagem e som - tela cheia, silenciar o jogo, volume e imagem da TV.",
        "estante - renomear os discos para o padrão No-Intro.",
        "conquistas - conta do RetroAchievements.",
        "sistema - baixar e atualizar o núcleo, checar atualizações do app ao abrir.",
        "bios - a BIOS usada (uma linha por arquivo de bios/).",
    ]);
    p.figura(
        &shot("config-imagem.png"),
        "Configurações > imagem e som.",
        520,
    );
    paginas.push(p.img);

    // 10 - Conquistas e cards.
    let mut p = Pagina::nova(10, Some("Conquistas e memory cards"));
    p.subtitulo("RetroAchievements");
    p.paragrafo(
        "Em Configurações > conquistas, informe seu usuário e a chave da web API do retroachievements.org e conecte a conta com a senha (ela só serve para conectar e não fica guardada). Com a conta conectada, o app identifica o jogo, mostra o progresso na estante e avisa cada conquista no queixo da TV.",
    );
    p.itens(&[
        "O modo hardcore bloqueia cheats e o Cheater Fish enquanto você ganha conquistas.",
        "Se a rede cair, as conquistas ficam numa fila e são reenviadas no próximo login.",
    ]);
    p.subtitulo("Memory cards");
    p.paragrafo(
        "Um card é um arquivo .mcr de 128 KB em memcards/. O app nunca grava por cima de um card que não conseguiu ler, e renomear um card nunca apaga outro. Faça cópia dos seus .mcr: é onde moram os seus saves.",
    );
    paginas.push(p.img);

    // 11 - Quando algo não funciona.
    let mut p = Pagina::nova(11, Some("Quando algo não funciona"));
    p.itens(&[
        "\"Para jogar é necessário baixar o núcleo\" - clique em \"Baixar núcleo\" na tela inicial ou em Configurações > sistema.",
        "O jogo não abre - confira se o disco ainda está em roms/ com o mesmo nome (use \"Atualizar\" na estante se você o moveu) e se há uma BIOS em bios/.",
        "Sem som - veja o volume da TV (VOL +), se a TV está ligada e se \"Silenciar o jogo\" está desligado.",
        "A imagem está escura ou lavada - o menu de imagem (MENU) tem brilho, contraste, cor e nitidez; 50 é o neutro.",
        "O jogo ficou lento - feche outros programas; o app mostra o aviso \"sem aceleração de GPU\" quando o vídeo cai para software.",
        "O app fechou - o motivo fica no arquivo crash.log da pasta do app e nos logs de config/logs.",
    ]);
    p.paragrafo(
        "Os logs ficam em config/logs, um por abertura. Ao reportar um problema, anexe o mais recente.",
    );
    paginas.push(p.img);

    // 12 - Contracapa.
    let mut p = Pagina::nova(12, Some("Sobre"));
    p.paragrafo(
        "PolystnX é software livre, sob a licença MIT. O núcleo PCSX Rearmed (GPL) e a BIOS do console não fazem parte do pacote; os cheats embutidos vêm do banco do libretro (CC BY-SA 4.0). Detalhes em THIRD-PARTY-NOTICES.md.",
    );
    p.paragrafo(&format!(
        "Versão {} - github.com/ticianocorral/PolystnX",
        env!("CARGO_PKG_VERSION")
    ));
    if p.restante() < 0 {
        anyhow::bail!("a página {} estourou", p.numero);
    }
    paginas.push(p.img);

    let jpgs: Vec<Vec<u8>> = paginas.iter().map(jpeg).collect();
    let pdf = escrever_pdf(&jpgs);
    std::fs::write(&saida, &pdf)?;
    println!(
        "{} páginas, {} KB → {}",
        jpgs.len(),
        pdf.len() / 1024,
        saida.display()
    );
    // Prévias das páginas para conferir (fora do repositório).
    if let Some(dir) = std::env::args().nth(1) {
        std::fs::create_dir_all(&dir)?;
        for (i, pg) in paginas.iter().enumerate() {
            pg.save(Path::new(&dir).join(format!("pagina-{:02}.png", i + 1)))?;
        }
    }
    Ok(())
}
