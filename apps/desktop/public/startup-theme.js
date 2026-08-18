(() => {
  const id = localStorage.getItem("app-tester.color-theme") || "default";
  const palettes = {
    "default":["#090d18","#eef0f8","#9b6cff"],"catppuccin-latte":["#eff1f5","#4c4f69","#8839ef"],"catppuccin-frappe":["#303446","#c6d0f5","#ca9ee6"],"catppuccin-macchiato":["#24273a","#cad3f5","#c6a0f6"],"catppuccin-mocha":["#1e1e2e","#cdd6f4","#cba6f7"],"dracula":["#21222c","#f8f8f2","#bd93f9"],"nord-dark":["#242933","#eceff4","#88c0d0"],"nord-light":["#eceff4","#2e3440","#5e81ac"],"gruvbox-dark":["#1d2021","#ebdbb2","#d3869b"],"gruvbox-light":["#fbf1c7","#3c3836","#8f3f71"],"tokyo-night":["#16161e","#c0caf5","#7aa2f7"],"tokyo-day":["#e9e9ec","#343b58","#34548a"],"rose-pine":["#191724","#e0def4","#c4a7e7"],"rose-pine-dawn":["#faf4ed","#575279","#907aa9"],"solarized-dark":["#002b36","#eee8d5","#268bd2"],"solarized-light":["#fdf6e3","#586e75","#268bd2"],"github-dark":["#0d1117","#e6edf3","#58a6ff"],"github-light":["#f6f8fa","#1f2328","#0969da"],"kanagawa":["#16161d","#dcd7ba","#957fb8"],"everforest":["#272e33","#d3c6aa","#a7c080"]
  };
  const palette = palettes[id] || palettes.default;
  document.documentElement.dataset.theme = id;
  document.documentElement.style.cssText = `--splash-bg:${palette[0]};--splash-text:${palette[1]};--splash-accent:${palette[2]};background:${palette[0]}`;
})();
