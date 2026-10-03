/* GeoSolver shared UI: icons, theme menu, math typesetting, typed facts,
 * the proof-step list and the figure viewer. Used by the app, the landing
 * page and the sign-in page. Exposes window.GS. */
(function () {
  "use strict";
  var t = function (k, v) { return window.i18n ? window.i18n.t(k, v) : k; };

  var P = 'fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"';
  var icons = {
    check: '<svg viewBox="0 0 24 24" ' + P + '><circle cx="12" cy="12" r="9.5"/><path d="m7.8 12.3 2.9 3 5.6-6.3"/></svg>',
    cross: '<svg viewBox="0 0 24 24" ' + P + '><circle cx="12" cy="12" r="9.5"/><path d="m8.5 8.5 7 7m0-7-7 7"/></svg>',
    approx: '<svg viewBox="0 0 24 24" ' + P + '><circle cx="12" cy="12" r="9.5"/><path d="M7.5 10.2c1.5-1.5 3-1.5 4.5 0s3 1.5 4.5 0M7.5 14.6c1.5-1.5 3-1.5 4.5 0s3 1.5 4.5 0"/></svg>',
    minus: '<svg viewBox="0 0 24 24" ' + P + '><circle cx="12" cy="12" r="9.5"/><path d="M8 12h8"/></svg>',
    clock: '<svg viewBox="0 0 24 24" ' + P + '><circle cx="12" cy="12" r="9.5"/><path d="M12 7v5l3.2 2"/></svg>',
    alert: '<svg viewBox="0 0 24 24" ' + P + '><path d="M10.3 3.9 2.4 17.5A2 2 0 0 0 4.1 20.5h15.8a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z"/><path d="M12 9v4.5M12 17h.01"/></svg>',
    info: '<svg viewBox="0 0 24 24" ' + P + '><circle cx="12" cy="12" r="9.5"/><path d="M12 11v5.5M12 7.6h.01"/></svg>',
    copy: '<svg viewBox="0 0 24 24" ' + P + '><rect x="8.5" y="8.5" width="12" height="12" rx="2"/><path d="M15.5 8.5V5.5a2 2 0 0 0-2-2h-8a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h3"/></svg>',
    download: '<svg viewBox="0 0 24 24" ' + P + '><path d="M12 3.5v12m-5-5 5 5 5-5M4.5 20.5h15"/></svg>',
    plus: '<svg viewBox="0 0 24 24" ' + P + '><path d="M12 5v14M5 12h14"/></svg>',
    minusSm: '<svg viewBox="0 0 24 24" ' + P + '><path d="M5 12h14"/></svg>',
    fit: '<svg viewBox="0 0 24 24" ' + P + '><circle cx="12" cy="12" r="3"/><path d="M12 2.5v3.5m0 12v3.5M2.5 12H6m12 0h3.5"/></svg>',
    expand: '<svg viewBox="0 0 24 24" ' + P + '><path d="M14.5 3.5h6v6m-6-6 6 6M9.5 20.5h-6v-6m6 6-6-6"/></svg>',
    collapse: '<svg viewBox="0 0 24 24" ' + P + '><path d="M20.5 3.5l-6 6m0-5v5h5M3.5 20.5l6-6m0 5v-5h-5"/></svg>',
    sun: '<svg viewBox="0 0 24 24" ' + P + '><circle cx="12" cy="12" r="4"/><path d="M12 2.5v2m0 15v2M4.6 4.6 6 6m12 12 1.4 1.4M2.5 12h2m15 0h2M4.6 19.4 6 18M18 6l1.4-1.4"/></svg>',
    moon: '<svg viewBox="0 0 24 24" ' + P + '><path d="M20.5 14.5A8.5 8.5 0 0 1 9.5 3.5a8.5 8.5 0 1 0 11 11Z"/></svg>',
    monitor: '<svg viewBox="0 0 24 24" ' + P + '><rect x="3" y="4" width="18" height="12.5" rx="2"/><path d="M8.5 20.5h7M12 16.5v4"/></svg>',
    history: '<svg viewBox="0 0 24 24" ' + P + '><path d="M3.5 12a8.5 8.5 0 1 0 2.5-6L3.5 8.5"/><path d="M3.5 3.5v5h5M12 7.5V12l3 2"/></svg>',
    search: '<svg viewBox="0 0 24 24" ' + P + '><circle cx="11" cy="11" r="6.5"/><path d="m20.5 20.5-4.8-4.8"/></svg>',
    trash: '<svg viewBox="0 0 24 24" ' + P + '><path d="M4 7h16M9.5 7V4.5h5V7M6.5 7l1 13h9l1-13M10 11v5.5M14 11v5.5"/></svg>',
    close: '<svg viewBox="0 0 24 24" ' + P + '><path d="m6 6 12 12M18 6 6 18"/></svg>',
    chevron: '<svg viewBox="0 0 24 24" ' + P + '><path d="m6.5 9.5 5.5 5.5 5.5-5.5"/></svg>',
    file: '<svg viewBox="0 0 24 24" ' + P + '><path d="M14 3.5H7a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-10l-5-5Z"/><path d="M14 3.5v5h5"/></svg>',
    image: '<svg viewBox="0 0 24 24" ' + P + '><rect x="3.5" y="4.5" width="17" height="15" rx="2"/><circle cx="9" cy="10" r="1.6"/><path d="m20.5 16-5-5-8.5 8.5"/></svg>',
    code: '<svg viewBox="0 0 24 24" ' + P + '><path d="m8.5 7.5-5 4.5 5 4.5m7-9 5 4.5-5 4.5"/></svg>',
    text: '<svg viewBox="0 0 24 24" ' + P + '><path d="M4.5 6.5h15M4.5 12h15M4.5 17.5h9"/></svg>',
    sparkle: '<svg viewBox="0 0 24 24" ' + P + '><path d="M12 3.5 13.8 9 19.5 10.8 13.8 12.6 12 18.5 10.2 12.6 4.5 10.8 10.2 9Z"/></svg>',
    book: '<svg viewBox="0 0 24 24" ' + P + '><path d="M4.5 5.5a2 2 0 0 1 2-2h12v15h-12a2 2 0 0 0-2 2v-15Z"/><path d="M4.5 20.5a2 2 0 0 1 2-2h12v2h-12"/></svg>',
    logout: '<svg viewBox="0 0 24 24" ' + P + '><path d="M9.5 20.5h-4a2 2 0 0 1-2-2v-13a2 2 0 0 1 2-2h4M16 16.5 20.5 12 16 7.5M20.5 12h-11"/></svg>',
    retry: '<svg viewBox="0 0 24 24" ' + P + '><path d="M20.5 12a8.5 8.5 0 1 1-2.5-6l2.5 2.5"/><path d="M20.5 3.5v5h-5"/></svg>',
    eye: '<svg viewBox="0 0 24 24" ' + P + '><path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Z"/><circle cx="12" cy="12" r="3"/></svg>',
    eyeOff: '<svg viewBox="0 0 24 24" ' + P + '><path d="M3.5 3.5l17 17M10.6 5.6A9.6 9.6 0 0 1 12 5.5c6 0 9.5 6.5 9.5 6.5a17 17 0 0 1-2.9 3.7M6.5 6.9C3.9 8.5 2.5 12 2.5 12S6 18.5 12 18.5a9 9 0 0 0 4.4-1.1M9.9 9.9a3 3 0 0 0 4.2 4.2"/></svg>',
    arrowRight: '<svg viewBox="0 0 24 24" ' + P + '><path d="M4.5 12h15m-6-6 6 6-6 6"/></svg>',
  };

  function esc(s) {
    return String(s == null ? "" : s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
  }

  var NAME_RE = /[A-ZΩω][a-z]?[₀-₉]*[′″]*/g;
  function nameHtml(m) {
    var low = /^([A-ZΩω])([a-z])(.*)$/.exec(m);
    return low ? "<i>" + low[1] + "<sub>" + low[2] + "</sub>" + low[3] + "</i>" : "<i>" + m + "</i>";
  }
  /** Escape `text` and set point names (capital letters with subscripts/primes)
   * in italic math type. In `prose` mode only all-capital tokens are treated
   * as names, so ordinary words keep their roman type. */
  function math(text, prose) {
    var s = String(text == null ? "" : text);
    if (!prose) return esc(s).replace(NAME_RE, nameHtml);
    return s.split(/(\s+|[(),.;:!?—–])/).map(function (tok) {
      if (/^[A-Z][A-Z₀-₉′″]*[²³]?$/.test(tok) || /^[A-Z][₀-₉′″]+$/.test(tok))
        return esc(tok).replace(/[A-ZΩω][₀-₉]*[′″]*/g, nameHtml);
      return esc(tok);
    }).join("");
  }

  /** A typed fact from the server (`{kind, args}`) as typeset, localized HTML. */
  function fact(f) {
    if (!f) return "";
    var a = (f.args || []).map(function (x) { return math(x); });
    var g = function (i) { return a[i] || ""; };
    switch (f.kind) {
      case "coll": return esc(t("fact.coll", { pts: "\u0001" })).replace("\u0001", a.join(", "));
      case "cyclic": return esc(t("fact.cyclic", { pts: "\u0001" })).replace("\u0001", a.join(", "));
      case "midp": return esc(t("fact.midp", { m: "\u0001", seg: "\u0002" })).replace("\u0001", g(0)).replace("\u0002", g(1));
      case "circle": return esc(t("fact.circle", { o: "\u0001", tri: "\u0002" })).replace("\u0001", g(0)).replace("\u0002", g(1));
      case "oncircle": return esc(t("fact.oncircle", { o: "\u0001", pts: "\u0002" })).replace("\u0001", g(0)).replace("\u0002", a.slice(1).join(", "));
      case "cong": case "length": case "eqangle": case "coincide": return g(0) + " = " + g(1);
      case "perp": return g(0) + " ⟂ " + g(1);
      case "para": return g(0) + " ∥ " + g(1);
      case "eqratio": return g(0) + " : " + g(1) + " = " + g(2) + " : " + g(3);
      case "aconst": return g(0) + " = " + g(1) + "°";
      case "rconst": return g(0) + " : " + g(1) + " = " + g(2);
      case "simtri": return "△" + g(0) + " ∼ △" + g(1);
      case "contri": return "△" + g(0) + " ≅ △" + g(1);
      case "eqdist": return a.join(" = ");
      case "points": return a.join(", ");
      case "prose": return math(((lang() === "ro" && f.ro) || f.args || [])[0], true);
      default: return a.join(" ");
    }
  }
  /** The same fact as plain text (for copying). */
  function factText(f) {
    var d = document.createElement("div");
    d.innerHTML = fact(f);
    return d.textContent;
  }

  function lang() { return window.i18n ? window.i18n.current() : "en"; }
  function ruleLabel(step) {
    if (step.rule === "theorem" && step.rule_name) return (lang() === "ro" && step.rule_name_ro) || step.rule_name;
    var k = "rule." + step.rule;
    return window.i18n && window.i18n.has(k) ? t(k) : (step.rule_name || step.rule);
  }

  /** Render the numbered, cited steps into `ol`. `hooks.focus(points)` is
   * called with the step's points on hover/focus (null on leave). */
  function renderSteps(ol, proof, hooks) {
    hooks = hooks || {};
    var steps = (proof && proof.steps) || [];
    var html = steps.map(function (s, i) {
      var cites = (s.deps || []).map(function (d) {
        return '<a class="cite" href="#step-' + d + '" data-step="' + d + '" tabindex="-1" aria-label="' + esc(t("proof.cite", { n: d })) + '">' + d + "</a>";
      }).join("");
      return '<li class="step' + (s.kind === "given" ? " is-given" : "") + '" id="step-' + s.n + '" data-n="' + s.n + '" data-points="' + esc((s.fact.points || []).join(" ")) + '" tabindex="' + (i === 0 ? 0 : -1) + '">' +
        '<span class="step-n" aria-hidden="true">' + s.n + "</span>" +
        '<div class="step-body"><div class="step-stmt math">' + fact(s.fact) + "</div>" +
        '<div class="step-meta"><span class="rule">' + esc(ruleLabel(s)) + "</span>" +
        (cites ? '<span class="cites"><span class="sr-only">' + esc(t("proof.from")) + " </span>" + cites + "</span>" : "") +
        "</div></div></li>";
    }).join("");
    if (proof && proof.conclusion) {
      html += '<li class="step is-conclusion" data-points="' + esc((proof.conclusion.points || []).join(" ")) + '" tabindex="-1"><span class="step-n" aria-hidden="true">∎</span><div class="step-body"><div class="step-stmt math"><span class="sr-only">' + esc(t("proof.conclusion")) + ": </span>" + fact(proof.conclusion) + "</div></div></li>";
    }
    ol.innerHTML = html;
    var items = Array.prototype.slice.call(ol.querySelectorAll(".step"));
    function pts(li) { var p = li.getAttribute("data-points"); return p ? p.split(" ") : []; }
    function activate(li) {
      items.forEach(function (x) {
        x.classList.toggle("is-active", x === li);
        x.querySelectorAll(".cite").forEach(function (c) { c.tabIndex = x === li ? 0 : -1; });
      });
      if (hooks.focus) hooks.focus(li ? pts(li) : null);
    }
    items.forEach(function (li, i) {
      li.addEventListener("mouseenter", function () { activate(li); });
      li.addEventListener("mouseleave", function () { if (document.activeElement !== li) activate(null); });
      li.addEventListener("focus", function () {
        items.forEach(function (x) { x.tabIndex = -1; });
        li.tabIndex = 0;
        activate(li);
      });
      li.addEventListener("keydown", function (e) {
        var j = null;
        if (e.key === "ArrowDown") j = Math.min(items.length - 1, i + 1);
        else if (e.key === "ArrowUp") j = Math.max(0, i - 1);
        else if (e.key === "Home") j = 0;
        else if (e.key === "End") j = items.length - 1;
        if (j != null && e.target === li) { e.preventDefault(); items[j].focus(); }
      });
    });
    if (ol._gsFocusOut) ol.removeEventListener("focusout", ol._gsFocusOut);
    ol._gsFocusOut = function (e) {
      if (!e.relatedTarget || !ol.contains(e.relatedTarget)) activate(null);
    };
    ol.addEventListener("focusout", ol._gsFocusOut);
    if (ol._gsClick) ol.removeEventListener("click", ol._gsClick);
    ol._gsClick = function (e) {
      var a = e.target.closest(".cite");
      if (!a) return;
      e.preventDefault();
      var target = ol.querySelector("#step-" + a.getAttribute("data-step"));
      if (!target) return;
      target.scrollIntoView({ block: "center", behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
      target.classList.remove("flash");
      void target.offsetWidth;
      target.classList.add("flash");
      target.focus({ preventScroll: true });
    };
    ol.addEventListener("click", ol._gsClick);
    return {
      markPoint: function (name) {
        items.forEach(function (li) { li.classList.toggle("uses-point", !!name && pts(li).indexOf(name) >= 0); });
      },
    };
  }

  /** Pan/zoom/highlight over an injected figure SVG. `els`: {frame, viewport,
   * zoomIn, zoomOut, fit, full, label}. */
  function Viewer(els) {
    this.els = els;
    this.svg = null;
    this.base = null;
    this.vb = null;
    this.onPoint = null;
    var me = this, vp = els.viewport;
    var pointers = new Map(), pinch = null, drag = null, lastTap = 0;

    function scale() {
      var r = vp.getBoundingClientRect();
      return Math.min(r.width / me.vb.w, r.height / me.vb.h) || 1;
    }
    this.scale = scale;
    vp.addEventListener("wheel", function (e) {
      if (!me.svg) return;
      e.preventDefault();
      me.zoomAt(e.clientX, e.clientY, Math.exp(-e.deltaY * 0.0016));
    }, { passive: false });
    vp.addEventListener("pointerdown", function (e) {
      if (!me.svg || e.button > 0) return;
      if (e.pointerType === "touch" && pointers.size === 0) {
        var now = Date.now();
        if (now - lastTap < 300) { me.zoomed() ? me.reset() : me.zoomAt(e.clientX, e.clientY, 2.5); lastTap = 0; return; }
        lastTap = now;
      }
      pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (pointers.size === 2) {
        var p = Array.from(pointers.values());
        pinch = { d: Math.hypot(p[0].x - p[1].x, p[0].y - p[1].y), mx: (p[0].x + p[1].x) / 2, my: (p[0].y + p[1].y) / 2 };
        drag = null;
      } else if (me.zoomed() || e.pointerType === "mouse") {
        drag = { x: e.clientX, y: e.clientY };
      }
      if (drag || pinch) { try { vp.setPointerCapture(e.pointerId); } catch (_) {} }
    });
    vp.addEventListener("pointermove", function (e) {
      if (!pointers.has(e.pointerId)) return;
      pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (pinch && pointers.size === 2) {
        var p = Array.from(pointers.values());
        var d = Math.hypot(p[0].x - p[1].x, p[0].y - p[1].y);
        var mx = (p[0].x + p[1].x) / 2, my = (p[0].y + p[1].y) / 2;
        me.panBy(mx - pinch.mx, my - pinch.my);
        me.zoomAt(mx, my, d / pinch.d);
        pinch = { d: d, mx: mx, my: my };
      } else if (drag) {
        if (!me.zoomed()) return;
        me.panBy(e.clientX - drag.x, e.clientY - drag.y);
        drag = { x: e.clientX, y: e.clientY };
        vp.classList.add("is-panning");
      }
    });
    function lift(e) {
      pointers.delete(e.pointerId);
      if (pointers.size < 2) pinch = null;
      if (pointers.size === 0) { drag = null; vp.classList.remove("is-panning"); }
    }
    vp.addEventListener("pointerup", lift);
    vp.addEventListener("pointercancel", lift);
    vp.addEventListener("dblclick", function (e) { if (!me.svg) return; e.preventDefault(); me.zoomed() ? me.reset() : me.zoomAt(e.clientX, e.clientY, 2.5); });
    vp.addEventListener("keydown", function (e) {
      if (!me.svg || e.altKey || e.ctrlKey || e.metaKey) return;
      var r = vp.getBoundingClientRect(), cx = r.left + r.width / 2, cy = r.top + r.height / 2;
      var handled = true;
      if (e.key === "+" || e.key === "=") me.zoomAt(cx, cy, 1.4);
      else if (e.key === "-" || e.key === "_") me.zoomAt(cx, cy, 1 / 1.4);
      else if (e.key === "0") me.reset();
      else if (e.key === "ArrowLeft") me.panBy(40, 0);
      else if (e.key === "ArrowRight") me.panBy(-40, 0);
      else if (e.key === "ArrowUp") me.panBy(0, 40);
      else if (e.key === "ArrowDown") me.panBy(0, -40);
      else if ((e.key === "f" || e.key === "F") && me.els.full) me.toggleFull();
      else if (e.key === "Escape" && me.isFull()) me.toggleFull(false);
      else handled = false;
      if (handled) e.preventDefault();
    });
    vp.addEventListener("pointerover", function (e) {
      var el = e.target.closest && e.target.closest(".f-lbl, .f-dot");
      if (me.onPoint) me.onPoint(el ? el.getAttribute("data-p") : null);
    });
    vp.addEventListener("pointerleave", function () { if (me.onPoint) me.onPoint(null); });
    function center() { var r = vp.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; }
    if (els.zoomIn) els.zoomIn.addEventListener("click", function () { var c = center(); me.zoomAt(c[0], c[1], 1.4); });
    if (els.zoomOut) els.zoomOut.addEventListener("click", function () { var c = center(); me.zoomAt(c[0], c[1], 1 / 1.4); });
    if (els.fit) els.fit.addEventListener("click", function () { me.reset(); });
    if (els.full) els.full.addEventListener("click", function () { me.toggleFull(); });
    document.addEventListener("keydown", function (e) { if (e.key === "Escape" && me.isFull()) me.toggleFull(false); });
    if (window.ResizeObserver) new ResizeObserver(function () { me.restyle(); }).observe(vp);
  }
  Viewer.prototype.setSvg = function (svgText, ariaLabel) {
    var vp = this.els.viewport;
    var old = vp.querySelector("svg");
    if (old) old.remove();
    if (svgText) vp.insertAdjacentHTML("beforeend", svgText);
    this.svg = vp.querySelector("svg");
    if (!this.svg) { this.base = this.vb = null; return; }
    var b = (this.svg.getAttribute("viewBox") || "0 0 100 100").split(/\s+/).map(Number);
    this.base = { x: b[0], y: b[1], w: b[2], h: b[3] };
    this.base0 = null;
    this.svg.removeAttribute("width");
    this.svg.removeAttribute("height");
    this.svg.setAttribute("preserveAspectRatio", "xMidYMid meet");
    this.svg.setAttribute("role", "img");
    this.svg.setAttribute("aria-label", ariaLabel || t("fig.title"));
    this.labels = Array.prototype.slice.call(this.svg.querySelectorAll(".f-lbl"));
    this.dots = Array.prototype.slice.call(this.svg.querySelectorAll(".f-dot"));
    this.reset();
  };
  Viewer.prototype.zoomed = function () { return this.vb && this.base && this.vb.w < this.base.w * 0.999; };
  Viewer.prototype.apply = function () {
    if (!this.svg) return;
    var v = this.vb;
    this.svg.setAttribute("viewBox", v.x.toFixed(2) + " " + v.y.toFixed(2) + " " + v.w.toFixed(2) + " " + v.h.toFixed(2));
    this.els.viewport.classList.toggle("is-zoomed", this.zoomed());
    if (this.els.label) {
      var z = Math.round((this.base.w / v.w) * 100);
      this.els.label.textContent = z + "%";
      this.els.label.hidden = z <= 100;
    }
    this.restyle();
  };
  /** Keep a figure's labels and dots a constant on-screen size while the
   * `vb` part of it is shown in a box of size `r`. */
  function fitLabels(svg, vb, r) {
    if (!svg || !vb || !r.width || !r.height) return;
    var s = Math.min(r.width / vb.w, r.height / vb.h);
    var fs = Math.max(13, Math.min(17, r.width / 28));
    var k = Math.max(0.35, Math.min(3, fs / s / 18));
    svg.style.setProperty("--lbl-fs", (18 * k).toFixed(2) + "px");
    svg.querySelectorAll(".f-lbl").forEach(function (el) {
      var x = +el.getAttribute("data-x"), y = +el.getAttribute("data-y"), dx = +el.getAttribute("data-dx"), dy = +el.getAttribute("data-dy");
      el.setAttribute("x", (x + dx * k).toFixed(1));
      el.setAttribute("y", (y + dy * k + 18 * k * 0.34).toFixed(1));
      el.setAttribute("stroke-width", (5 * k).toFixed(2));
    });
    var dotR = Math.max(1.2, Math.min(8, (r.width < 500 ? 3.2 : 3.8) / s));
    svg.querySelectorAll(".f-dot").forEach(function (el) { el.setAttribute("r", dotR.toFixed(2)); });
  }
  Viewer.prototype.restyle = function () {
    if (!this.svg || !this.vb) return;
    fitLabels(this.svg, this.vb, this.els.viewport.getBoundingClientRect());
  };
  Viewer.prototype.reset = function () {
    if (!this.base) return;
    var b0 = this.base0 || (this.base0 = { x: this.base.x, y: this.base.y, w: this.base.w, h: this.base.h });
    var r = this.els.viewport.getBoundingClientRect();
    if (r.width && r.height) {
      var s0 = Math.min(r.width / b0.w, r.height / b0.h);
      var k = Math.max(0.35, Math.min(3, Math.max(13, Math.min(17, r.width / 28)) / s0 / 18));
      var pad = Math.max(0, (k - 1) * 32);
      this.base = { x: b0.x - pad, y: b0.y - pad, w: b0.w + 2 * pad, h: b0.h + 2 * pad };
    }
    this.vb = { x: this.base.x, y: this.base.y, w: this.base.w, h: this.base.h };
    this.apply();
  };
  Viewer.prototype.clamp = function () {
    var b = this.base, v = this.vb;
    var cx = Math.min(b.x + b.w, Math.max(b.x, v.x + v.w / 2));
    var cy = Math.min(b.y + b.h, Math.max(b.y, v.y + v.h / 2));
    v.x = cx - v.w / 2;
    v.y = cy - v.h / 2;
  };
  Viewer.prototype.zoomAt = function (clientX, clientY, k) {
    if (!this.svg) return;
    var v = this.vb, b = this.base;
    var nw = Math.min(b.w, Math.max(b.w / 14, v.w / k));
    k = v.w / nw;
    if (Math.abs(k - 1) < 1e-6) return;
    var ctm = this.svg.getScreenCTM();
    if (!ctm) return;
    var pt = this.svg.createSVGPoint();
    pt.x = clientX; pt.y = clientY;
    var u = pt.matrixTransform(ctm.inverse());
    v.x = u.x - (u.x - v.x) / k;
    v.y = u.y - (u.y - v.y) / k;
    v.w = v.w / k;
    v.h = v.h / k;
    if (v.w >= b.w * 0.999) { this.reset(); return; }
    this.clamp();
    this.apply();
  };
  Viewer.prototype.panBy = function (dx, dy) {
    if (!this.svg || !this.zoomed()) return;
    var s = this.scale();
    this.vb.x -= dx / s;
    this.vb.y -= dy / s;
    this.clamp();
    this.apply();
  };
  Viewer.prototype.isFull = function () { return this.els.frame.classList.contains("is-full"); };
  Viewer.prototype.toggleFull = function (on) {
    var f = this.els.frame;
    if (!this.els.full) return;
    on = on == null ? !this.isFull() : on;
    if (on === this.isFull()) return;
    f.classList.toggle("is-full", on);
    document.documentElement.classList.toggle("no-scroll", on);
    modal(f, on, t("fig.title"));
    if (!on) this.els.full.focus({ preventScroll: true });
    if (this.els.full) {
      this.els.full.innerHTML = on ? icons.collapse : icons.expand;
      var label = t(on ? "fig.exit_full" : "fig.full");
      this.els.full.setAttribute("aria-label", label);
      this.els.full.setAttribute("title", label);
      this.els.full.setAttribute("aria-pressed", on ? "true" : "false");
    }
    var me = this;
    requestAnimationFrame(function () { me.reset(); });
    if (on) this.els.viewport.focus({ preventScroll: true });
  };

  /** Make `el` modal (everything outside it inert, `el` a labelled dialog);
   * `on` false restores exactly what was made inert. */
  var inerted = new Map();
  function trapTab(e) {
    if (e.key !== "Tab") return;
    var f = Array.prototype.filter.call(e.currentTarget.querySelectorAll("button, [href], input, textarea, select, [tabindex]"), function (x) {
      return x.tabIndex >= 0 && !x.disabled && x.offsetParent !== null;
    });
    if (!f.length) return;
    var first = f[0], last = f[f.length - 1];
    if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
  }
  function modal(el, on, label) {
    if (on) {
      var list = [];
      for (var n = el; n && n.parentElement && n !== document.body; n = n.parentElement) {
        Array.prototype.forEach.call(n.parentElement.children, function (sib) {
          if (sib !== n && !sib.inert && sib.tagName !== "SCRIPT" && sib.id !== "toasts" && sib.id !== "announce") { sib.inert = true; list.push(sib); }
        });
      }
      inerted.set(el, list);
      el.addEventListener("keydown", trapTab);
      el.setAttribute("role", "dialog");
      el.setAttribute("aria-modal", "true");
      if (label) el.setAttribute("aria-label", label);
    } else {
      (inerted.get(el) || []).forEach(function (x) { x.inert = false; });
      inerted.delete(el);
      el.removeEventListener("keydown", trapTab);
      el.removeAttribute("aria-modal");
      el.removeAttribute("role");
      el.removeAttribute("aria-label");
    }
  }
  /** Emphasise the elements a set of points defines; null clears. */
  Viewer.prototype.highlight = function (points) {
    if (!this.svg) return;
    var S = points && points.length ? new Set(points) : null;
    this.svg.classList.toggle("has-focus", !!S);
    this.svg.querySelectorAll("[data-p]").forEach(function (el) {
      if (!S) { el.classList.remove("hl"); return; }
      var pts = el.getAttribute("data-p").split(" ");
      var n = 0;
      pts.forEach(function (p) { if (S.has(p)) n++; });
      var on;
      if (el.classList.contains("f-dot") || el.classList.contains("f-lbl")) on = S.has(pts[0]);
      else if (el.tagName === "line") on = n >= 2;
      else if (el.tagName === "circle") { var c = el.getAttribute("data-c"); on = n >= 3 || (c && S.has(c) && n >= 2); }
      else on = n === pts.length;
      el.classList.toggle("hl", !!on);
    });
  };

  var THEMES = ["system", "light", "dark"];
  function themeNow() {
    try { var v = localStorage.getItem("theme"); return THEMES.indexOf(v) > 0 ? v : "system"; } catch (e) { return "system"; }
  }
  function applyTheme(mode) {
    var root = document.documentElement;
    if (mode === "system") root.removeAttribute("data-theme"); else root.setAttribute("data-theme", mode);
    try { if (mode === "system") localStorage.removeItem("theme"); else localStorage.setItem("theme", mode); } catch (e) {}
    document.querySelectorAll("[data-theme-toggle]").forEach(paintThemeButton);
    var dark = mode === "dark" || (mode === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
    var m = document.querySelector('meta[name="theme-color"]');
    if (m) m.setAttribute("content", dark ? "#121417" : "#f7f7f5");
  }
  function paintThemeButton(b) {
    var mode = themeNow();
    b.innerHTML = mode === "light" ? icons.sun : mode === "dark" ? icons.moon : icons.monitor;
    var label = t("theme.label", { mode: t("theme." + mode) });
    b.setAttribute("aria-label", label);
    b.setAttribute("title", label);
  }
  function initTheme() {
    document.querySelectorAll("[data-theme-toggle]").forEach(function (b) {
      paintThemeButton(b);
      b.addEventListener("click", function () {
        applyTheme(THEMES[(THEMES.indexOf(themeNow()) + 1) % THEMES.length]);
      });
    });
    applyTheme(themeNow());
    try { matchMedia("(prefers-color-scheme: dark)").addEventListener("change", function () { applyTheme(themeNow()); }); } catch (e) {}
    document.addEventListener("langchange", function () { document.querySelectorAll("[data-theme-toggle]").forEach(paintThemeButton); });
  }

  function fmtSecs(x) {
    var n = window.i18n ? window.i18n.fmtNum : function (v) { return String(v); };
    if (x == null) return "";
    if (x < 0.001) return "< 1 ms";
    if (x < 1) return n(Math.round(x * 1000)) + " ms";
    if (x < 10) return n(x, 1) + " s";
    return n(Math.round(x)) + " s";
  }

  function toast(msg, opts) {
    opts = opts || {};
    var region = document.getElementById("toasts");
    if (!region) return null;
    var el = document.createElement("div");
    el.className = "toast";
    el.innerHTML = '<div class="grow"></div>';
    el.firstChild.textContent = msg;
    var done = false;
    function close() { if (done) return; done = true; el.remove(); if (opts.onClose) opts.onClose(); }
    if (opts.action) {
      var b = document.createElement("button");
      b.type = "button";
      b.className = "btn btn-sm";
      b.textContent = opts.action;
      b.addEventListener("click", function () { done = true; el.remove(); opts.onAction(); });
      el.appendChild(b);
    }
    region.appendChild(el);
    var ms = opts.ms || 4000, timer = setTimeout(close, ms);
    function resume() {
      if (done || el.contains(document.activeElement) || el.matches(":hover")) return;
      clearTimeout(timer);
      timer = setTimeout(close, ms);
    }
    el.addEventListener("mouseenter", function () { clearTimeout(timer); });
    el.addEventListener("focusin", function () { clearTimeout(timer); });
    el.addEventListener("mouseleave", resume);
    el.addEventListener("focusout", function () { setTimeout(resume, 0); });
    return { close: close, el: el, button: el.querySelector("button") };
  }

  window.GS = { icons: icons, esc: esc, math: math, fact: fact, factText: factText, ruleLabel: ruleLabel, renderSteps: renderSteps, Viewer: Viewer, fitLabels: fitLabels, modal: modal, initTheme: initTheme, fmtSecs: fmtSecs, toast: toast };
})();
