//! Manuais em PDF (plan revision: "colocar suporte para abrir manual pdf /
//! criar pasta para colocar manual / abrir na tv como o back cover com
//! opção de ver as paginas") — `assets/manual/<nome da rom>.pdf`, aberto
//! no tubo da estante com navegação de páginas.
//!
//! Renderização: os manuais de ROM que circulam são **escaneados** — cada
//! página é uma imagem grande embutida no PDF. Em vez de um renderizador de
//! PDF completo (dependência pesada em todas as plataformas), o módulo
//! extrai, por página, a **maior imagem** dos seus XObjects: JPEG
//! (`DCTDecode`) decodifica direto pelo `image`; bitmap comprimido
//! (`FlateDecode`, DeviceRGB/DeviceGray 8bpc) é reconstruído dos samples.
//! Página sem imagem embutida (PDF vetorial/texto) não tem visual próprio —
//! o leitor mostra "página não disponível".

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use lopdf::{Document as PdfDocument, Object, Stream};

/// Decodificado para o leitor: RGBA + dimensões (já reduzido para caber).
pub struct PageImage {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// Worker → estante: a contagem de páginas ao abrir e cada página
/// renderizada sob demanda (`w == 0` = página sem imagem embutida).
pub enum ManualEv {
    Ready {
        sha1: String,
        pages: usize,
    },
    Page {
        sha1: String,
        page: usize,
        w: u32,
        h: u32,
        rgba: Vec<u8>,
    },
}

/// Quantas páginas o PDF em `path` tem.
pub fn page_count(path: &Path) -> Result<usize> {
    let doc = PdfDocument::load(path).map_err(|e| anyhow!("pdf: {e}"))?;
    Ok(doc.get_pages().len())
}

/// A imagem da página `page` (1-based), reduzida para caber em `max` px.
/// `None` = página sem imagem embutida (ou ilegível) — o leitor trata como
/// "página não disponível", não como erro.
pub fn render_page(path: &Path, page: usize, max: u32) -> Result<Option<PageImage>> {
    let doc = PdfDocument::load(path).map_err(|e| anyhow!("pdf: {e}"))?;
    let pages = doc.get_pages();
    let Some((&num, &page_id)) = pages.iter().nth(page.saturating_sub(1)) else {
        return Ok(None);
    };
    let _ = num;
    let Some((w, h, bytes)) = largest_page_image(&doc, page_id) else {
        return Ok(None);
    };
    let img = image::load_from_memory(&bytes)
        .ok()
        .or_else(|| raw_to_image(w, h, &bytes))
        .context("decodificando a imagem da página")?;
    let img = img.thumbnail(max, max).to_rgba8();
    Ok(Some(PageImage {
        w: img.width(),
        h: img.height(),
        rgba: img.into_raw(),
    }))
}

/// A maior imagem embutida da página — o scan em si. `(w, h, bytes crus)`,
/// onde `bytes` é um JPEG quando o filtro é `DCTDecode` (o `image` resolve)
/// e samples crus 8bpc quando `FlateDecode` (o `raw_to_image` reconstrói).
fn largest_page_image(doc: &PdfDocument, page_id: lopdf::ObjectId) -> Option<(i64, i64, Vec<u8>)> {
    // A /Resources pode estar HERDADA do nó /Pages pai (padrão em PDFs de
    // scanner) — sobe a árvore procurando; e as imagens costumam morar
    // todas num dicionário compartilhado, com o content stream da página
    // escolhendo a sua (`/Im0 Do`). Só cai no "maior do dicionário" quando
    // o content não nomeia nada.
    let resources = page_resources(doc, page_id)?;
    let xobjects = get_resolve(doc, &resources, b"XObject")?;
    let Object::Dictionary(xdict) = &xobjects else {
        return None;
    };
    let invoked = content_image_names(doc, page_id);

    let mut best: Option<(i64, i64, Vec<u8>)> = None;
    for (name, obj) in xdict.iter() {
        if !invoked.is_empty() && !invoked.contains(&name.to_vec()) {
            continue;
        }
        let Some(obj) = resolve(doc, obj) else {
            continue;
        };
        let Ok(stream) = obj.as_stream() else {
            continue;
        };
        let dict_obj = Object::Dictionary(stream.dict.clone());
        if get_resolve(doc, &dict_obj, b"Subtype")
            .and_then(|o| o.as_name().ok().map(|n| n.to_vec()))
            .as_deref()
            != Some(b"Image".as_slice())
        {
            continue;
        }
        let Some(w) = get_resolve(doc, &dict_obj, b"Width").and_then(|o| int_of(&o)) else {
            continue;
        };
        let Some(h) = get_resolve(doc, &dict_obj, b"Height").and_then(|o| int_of(&o)) else {
            continue;
        };
        if w <= 0 || h <= 0 {
            continue;
        }
        let Some(data) = stream_data(doc, stream) else {
            continue;
        };
        let better = match &best {
            Some((bw, bh, _)) => w * h > bw * bh,
            None => true,
        };
        if better {
            best = Some((w, h, data));
        }
    }
    best
}

/// A /Resources da página, subindo a árvore /Parent quando herdada.
fn page_resources(doc: &PdfDocument, page_id: lopdf::ObjectId) -> Option<Object> {
    let mut cur = doc.get_object(page_id).ok()?.clone();
    for _ in 0..16 {
        if let Some(res) = get_resolve(doc, &cur, b"Resources") {
            return Some(res);
        }
        cur = get_resolve(doc, &cur, b"Parent")?;
    }
    None
}

/// Os nomes de XObject que o content da página desenha (`/Nome Do`), em
/// ordem — é o que diz qual imagem é a PÁGINA quando o dicionário de
/// recursos é compartilhado por todas.
fn content_image_names(doc: &PdfDocument, page_id: lopdf::ObjectId) -> Vec<Vec<u8>> {
    // lopdf devolve a LISTA de content streams da página — concatena os
    // decodificados e procura os operadores `Do`.
    let contents = doc.get_page_contents(page_id);
    let mut contents_all = Vec::new();
    for (num, gen) in contents {
        let Ok(obj) = doc.get_object((num, gen)) else {
            continue;
        };
        let Ok(stream) = obj.as_stream() else {
            continue;
        };
        if let Some(data) = stream_data(doc, stream) {
            contents_all.extend(data);
        }
    }
    let contents = contents_all;
    let mut names = Vec::new();
    let mut i = 0;
    while i + 4 <= contents.len() {
        if contents[i] == b'/' {
            // nome = '/' + token até espaço; procura o operador `Do` depois.
            let name_start = i + 1;
            let mut j = name_start;
            while j < contents.len() && !contents[j].is_ascii_whitespace() {
                j += 1;
            }
            let mut k = j;
            while k < contents.len() && contents[k].is_ascii_whitespace() {
                k += 1;
            }
            if contents.len() >= k + 2 && &contents[k..k + 2] == b"Do" {
                names.push(contents[name_start..j].to_vec());
                i = k + 2;
                continue;
            }
        }
        i += 1;
    }
    names
}

/// Os bytes decodificados da stream como o `image` os entende: JPEG
/// (`DCTDecode`) passa cru (o filtro não descomprime — é o JPEG em si);
/// `FlateDecode` descomprime; sem filtro, cru.
fn stream_data(doc: &PdfDocument, stream: &Stream) -> Option<Vec<u8>> {
    let filter_obj = get_resolve(doc, &Object::Dictionary(stream.dict.clone()), b"Filter");
    let filters: Vec<Vec<u8>> = match filter_obj {
        Some(Object::Name(n)) => vec![n.clone()],
        Some(Object::Array(a)) => a
            .iter()
            .filter_map(|o| o.as_name().ok().map(|n| n.to_vec()))
            .collect(),
        _ => Vec::new(),
    };

    // Todos os filtros são aplicados em ordem — só o caminho de um filtro
    // é suportado (scans usam exatamente um).
    if let [f] = filters.as_slice() {
        if f.as_slice() == b"FlateDecode" {
            let mut s = stream.clone();
            s.decompress().ok()?;
            return Some(s.content);
        }
        if f.as_slice() == b"DCTDecode" {
            return Some(stream.content.clone());
        }
        return None;
    }
    if filters.is_empty() {
        return Some(stream.content.clone());
    }
    None
}

/// Samples crus 8bpc → imagem (o caminho `FlateDecode` sem JPEG).
fn raw_to_image(w: i64, h: i64, samples: &[u8]) -> Option<image::DynamicImage> {
    let (w, h) = (w as u32, h as u32);
    if let Some(rgb) = image::RgbImage::from_raw(w, h, samples.to_vec()) {
        return Some(image::DynamicImage::ImageRgb8(rgb));
    }
    if let Some(gray) = image::GrayImage::from_raw(w, h, samples.to_vec()) {
        return Some(image::DynamicImage::ImageLuma8(gray));
    }
    None
}

fn resolve<'a>(doc: &'a PdfDocument, obj: &'a Object) -> Option<&'a Object> {
    match obj {
        Object::Reference(id) => doc.get_object(*id).ok(),
        other => Some(other),
    }
}

/// `dict[key]` resolvidos por referência — o shape do lopdf espalha
/// indireções em todo PDF real.
fn get_resolve<'a>(doc: &'a PdfDocument, obj: &'a Object, key: &[u8]) -> Option<Object> {
    let dict = obj.as_dict().ok()?;
    let found = dict.get(key).ok()?;
    resolve(doc, found).cloned()
}

fn int_of(obj: &Object) -> Option<i64> {
    obj.as_i64().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Monta um PDF mínimo de 2 páginas, cada uma com uma imagem RGB 2×2
    /// sem compressão, e confere contagem + extração (o caminho que o
    /// leitor percorre para manuais escaneados, menos o JPEG).
    #[test]
    fn counts_pages_and_extracts_embedded_page_images() {
        // Objetos: 1 catalog, 2 pages, 3/4 page, 5/6 imagem
        let img_bytes = |r: u8, g: u8, b: u8| vec![r, g, b, r, g, b, r, g, b, r, g, b];
        let page_img = |img_ref: u32| {
            format!(
                "<< /Type /Page /Parent 2 0 R /Resources << /XObject << /Im0 {img_ref} 0 R >> >> /Contents << >> >>"
            )
        };
        let img_obj = |r: u8, g: u8, b: u8| -> Vec<u8> {
            let mut out = format!(
                "<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {} >>\nstream\n",
                img_bytes(r, g, b).len()
            )
            .into_bytes();
            out.extend(img_bytes(r, g, b));
            out.extend(b"\nendstream");
            out
        };

        let objects: Vec<Vec<u8>> = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_vec(),
            page_img(5).into_bytes(),
            page_img(6).into_bytes(),
            img_obj(255, 0, 0),
            img_obj(0, 0, 255),
        ];

        // Monta o arquivo com xref correto (binário seguro).
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend(format!("{} 0 obj\n", i + 1).into_bytes());
            pdf.extend(body);
            pdf.extend(b"\nendobj\n");
        }
        let xref_at = pdf.len();
        pdf.extend(format!("xref\n0 {}\n", objects.len() + 1).into_bytes());
        pdf.extend(b"0000000000 65535 f \n");
        for off in &offsets {
            pdf.extend(format!("{off:010} 00000 n \n").into_bytes());
        }
        pdf.extend(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                objects.len() + 1,
                xref_at
            )
            .into_bytes(),
        );

        let dir = std::env::temp_dir().join("xperience-manual-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("manual.pdf");
        std::fs::write(&path, pdf).unwrap();

        assert_eq!(page_count(&path).unwrap(), 2);
        let p1 = render_page(&path, 1, 2).unwrap().unwrap();
        assert_eq!((p1.w, p1.h), (2, 2));
        // A página 1 é a vermelha (RGBA).
        assert_eq!(&p1.rgba[..4], &[255, 0, 0, 255]);
        let p2 = render_page(&path, 2, 2).unwrap().unwrap();
        assert_eq!(&p2.rgba[..4], &[0, 0, 255, 255]);
        // Fora do intervalo: sem imagem, sem erro.
        assert!(render_page(&path, 3, 2).unwrap().is_none());
    }
}
