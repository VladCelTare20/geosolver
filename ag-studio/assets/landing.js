/* Landing page: the real IMO 2023 P2 result (assets/showcase.json) with an
 * interactive figure and its verified steps. */
(function () {
  "use strict";
  var $ = function (id) { return document.getElementById(id); };
  var t = function (k, v) { return window.i18n.t(k, v); };
  var FIRST = 12;
  var data = null, expanded = false, viewer = null;

  document.querySelectorAll("[data-icon]").forEach(function (el) { el.innerHTML = GS.icons[el.getAttribute("data-icon")] || ""; });
  $("z-in").innerHTML = GS.icons.plus;
  $("z-out").innerHTML = GS.icons.minusSm;
  $("z-fit").innerHTML = GS.icons.fit;

  function paintSteps() {
    if (!data) return;
    var proof = data.view.proof;
    var steps = expanded ? proof.steps : proof.steps.slice(0, FIRST);
    GS.renderSteps($("steps"), { steps: steps, conclusion: expanded ? proof.conclusion : null }, {
      focus: function (pts) { viewer.highlight(pts); },
    });
    var more = $("more");
    more.hidden = proof.steps.length <= FIRST;
    more.textContent = expanded ? t("show.fewer") : t("show.steps", { n: proof.steps.length });
    more.setAttribute("aria-expanded", expanded ? "true" : "false");
  }

  function paintStats() {
    if (!data) return;
    var n = window.i18n.fmtNum;
    $("st-first").textContent = n(0.9, 1) + " s";
    $("st-steps").textContent = n(data.view.proof.steps.length);
    $("st-aux").textContent = n((data.aux_constructions || []).length);
    $("st-examined").textContent = n(data.examined || 0);
  }

  $("more").addEventListener("click", function () {
    expanded = !expanded;
    paintSteps();
    if (!expanded) $("showcase").scrollIntoView({ block: "start" });
  });

  fetch("/assets/showcase.json").then(function (r) { return r.json(); }).then(function (d) {
    data = d;
    $("hero-fig").innerHTML = d.svg;
    var hs = $("hero-fig").querySelector("svg");
    hs.removeAttribute("width");
    hs.removeAttribute("height");
    hs.setAttribute("aria-hidden", "true");
    hs.style.setProperty("--lbl-fs", "22px");
    viewer = new GS.Viewer({ frame: $("case-fig").parentNode, viewport: $("case-fig"), zoomIn: $("z-in"), zoomOut: $("z-out"), fit: $("z-fit") });
    viewer.setSvg(d.svg, t("case.title"));
    paintStats();
    paintSteps();
  }).catch(function () {});

  document.addEventListener("langchange", function () { paintStats(); paintSteps(); });
  window.i18n.apply();
  GS.initTheme();
})();
