// Retro Xperience — site estático: pouco JavaScript de propósito.

// Marca JS para o CSS só animar quando dá para animar.
document.documentElement.classList.add('js');

// Ano do rodapé.
const ano = document.getElementById('ano');
if (ano) ano.textContent = String(new Date().getFullYear());

// Revela cartões ao entrar na tela.
const reveals = document.querySelectorAll('.reveal');
if ('IntersectionObserver' in window && reveals.length > 0) {
  const observador = new IntersectionObserver(
    (entradas) => {
      for (const entrada of entradas) {
        if (entrada.isIntersecting) {
          entrada.target.classList.add('on');
          observador.unobserve(entrada.target);
        }
      }
    },
    { threshold: 0.15 }
  );
  reveals.forEach((el) => observador.observe(el));
} else {
  reveals.forEach((el) => el.classList.add('on'));
}

// O tubo liga e desliga com clique, como um CRT de verdade.
const tubo = document.querySelector('.crt.hero-midia');
if (tubo) {
  tubo.setAttribute('role', 'button');
  tubo.setAttribute('tabindex', '0');
  tubo.setAttribute('aria-pressed', 'false');
  const alternar = () => {
    const off = tubo.classList.toggle('off');
    tubo.setAttribute('aria-pressed', String(off));
  };
  tubo.addEventListener('click', alternar);
  tubo.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      alternar();
    }
  });
}
