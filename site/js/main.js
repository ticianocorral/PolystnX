// PolyStnX — manual: interações mínimas.

// Ano do rodapé.
document.getElementById("ano").textContent = new Date().getFullYear();

// Revelação sutil ao rolar (figuras, cartões e cabeçalhos de seção).
const alvos = document.querySelectorAll(
  ".figura, .passo, .lateral, .aviso, .sumario-lista, .tabela"
);
alvos.forEach((el) => el.classList.add("reveal-in"));
const io = new IntersectionObserver(
  (entradas) => {
    for (const e of entradas) {
      if (e.isIntersecting) {
        e.target.classList.add("reveal-ok");
        io.unobserve(e.target);
      }
    }
  },
  { threshold: 0.15 }
);
alvos.forEach((el) => io.observe(el));

// Versão real da última release (o texto fixo no HTML é o fallback).
const selo = document.querySelector("[data-release]");
if (selo) {
  fetch(
    "https://api.github.com/repos/ticianocorral/PolystnX/releases/latest"
  )
    .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
    .then((r) => {
      const tag = String(r.tag_name || "").replace(/^v/, "");
      if (tag) selo.textContent = tag;
    })
    .catch(() => {}); // sem rede ou rate limit: fica o valor do HTML
}

// Código Konami no site também — claro que sim. O bannerzinho fica lá
// embaixo convidando; quem completa vê a mensagem virar.
const konami = [
  "ArrowUp", "ArrowUp", "ArrowDown", "ArrowDown",
  "ArrowLeft", "ArrowRight", "ArrowLeft", "ArrowRight", "b", "a",
];
let passo = 0;
document.addEventListener("keydown", (ev) => {
  const ok = ev.key === konami[passo] || ev.key.toLowerCase() === konami[passo];
  passo = ok ? passo + 1 : ev.key === konami[0] ? 1 : 0;
  if (passo === konami.length) {
    passo = 0;
    const banner = document.getElementById("konami");
    if (banner) {
      banner.textContent = "código Konami aceito — no app, isso libera o devmode ↑↑↓↓←→←→BA";
      banner.classList.add("konami-ok");
    }
  }
});
