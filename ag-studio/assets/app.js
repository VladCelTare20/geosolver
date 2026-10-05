/* GeoSolver app: composer, solving flow, verdict, proof, figure, history. */
(function () {
  "use strict";
  var $ = function (id) { return document.getElementById(id); };
  var t = function (k, v) { return window.i18n.t(k, v); };
  var tp = function (k, n, v) { return window.i18n.tp(k, n, v); };
  var esc = GS.esc, icons = GS.icons;
  var isMac = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
  var store = {
    get: function (k) { try { return localStorage.getItem(k); } catch (e) { return null; } },
    set: function (k, v) { try { localStorage.setItem(k, v); } catch (e) {} },
    del: function (k) { try { localStorage.removeItem(k); } catch (e) {} },
  };

  var EXAMPLES = [
    { id: "ortho", en: "Orthocenter reflection", ro: "Simetricul ortocentrului", tag: "cyclic",
      c: { en: "Orthocenter reflection: H reflected in BC\nlies on the circumcircle.", ro: "Simetricul ortocentrului: simetricul lui H\nfață de BC se află pe cercul circumscris." },
      body: "A B C = triangle\nH = orthocenter(A, B, C)\nprove cyclic(A, B, C, reflect(H, line(B, C)))" },
    { id: "euler", en: "Euler line", ro: "Dreapta lui Euler", tag: "coll",
      c: { en: "Euler line: circumcenter, centroid and orthocenter are collinear.", ro: "Dreapta lui Euler: centrul cercului circumscris, centrul de greutate și ortocentrul sunt coliniare." },
      body: "A B C = triangle\nO = circumcenter(A, B, C)\nG = centroid(A, B, C)\nH = orthocenter(A, B, C)\nprove coll(O, G, H)" },
    { id: "nine", en: "Nine-point circle", ro: "Cercul lui Euler", tag: "cyclic",
      c: { en: "Nine-point circle: the side midpoints and an altitude foot are concyclic.", ro: "Cercul lui Euler: mijloacele laturilor și piciorul unei înălțimi sunt conciclice." },
      body: "A B C = triangle\nMa = midpoint(B, C)\nMb = midpoint(A, C)\nMc = midpoint(A, B)\nF = foot(A, line(B, C))\nprove cyclic(Ma, Mb, Mc, F)" },
    { id: "simson", en: "Simson line", ro: "Dreapta lui Simson", tag: "coll",
      c: { en: "Simson line: the feet of the perpendiculars from a point on the circumcircle are collinear.", ro: "Dreapta lui Simson: picioarele perpendicularelor duse dintr-un punct al cercului circumscris sunt coliniare." },
      body: "A B C = triangle\nP = on_circum(A, B, C)\nX = foot(P, line(B, C))\nY = foot(P, line(C, A))\nZ = foot(P, line(A, B))\nprove coll(X, Y, Z)" },
    { id: "stewart", en: "Stewart's theorem", ro: "Teorema lui Stewart", tag: "metric",
      c: { en: "Stewart's theorem: in triangle ABC with AB = 5, AC = 4, BC = 6 and BD:DC = 1:2, AD² = 14.", ro: "Teorema lui Stewart: în triunghiul ABC cu AB = 5, AC = 4, BC = 6 și BD:DC = 1:2, AD² = 14." },
      body: "B = free\nC = point: dist(B,C)=6\nD = point: coll(B,D,C), dist(B,D)=2\nA = point: dist(A,B)=5, dist(A,C)=4\nprove dist(A,D)^2 = 14" },
    { id: "pyth", en: "Pythagorean theorem", ro: "Teorema lui Pitagora", tag: "metric",
      c: { en: "Pythagorean theorem", ro: "Teorema lui Pitagora" },
      body: "B C = segment\nA = on_dia(B, C)\nprove dist(A, B)^2 + dist(A, C)^2 = dist(B, C)^2" },
    { id: "imo2019", en: "IMO 2019 · Problem 2", ro: "IMO 2019 · Problema 2", tag: "IMO",
      c: { en: "IMO 2019 Problem 2", ro: "IMO 2019, Problema 2" },
      body: "A B C = triangle\nA1 = point: coll(B, C, A1)\nB1 = point: coll(A, C, B1)\nP = point: coll(A, A1, P)\nQ = point: coll(B, B1, Q), para(P, Q, A, B)\nP1 = point: coll(B1, P, P1), eqangle(P1, P, P1, C, A, B, A, C)\nQ1 = point: coll(A1, Q, Q1), eqangle(Q1, C, Q1, Q, B, C, B, A)\nprove cyclic(P, Q, P1, Q1)" },
    { id: "imo2023", en: "IMO 2023 · Problem 2", ro: "IMO 2023 · Problema 2", tag: "IMO",
      c: { en: "IMO 2023 Problem 2", ro: "IMO 2023, Problema 2" },
      body: "A B C = triangle\nO = circumcenter(A, B, C)\nI = incenter(A, B, C)\nN = meet(line(A, I), circumcircle(A, B, C))\nS = meet(line(N, O), circumcircle(A, B, C))\nD = point: coll(B, S, D), perp(A, D, B, C)\nE = meet(line(A, D), circumcircle(A, B, C))\nL = point: coll(B, E, L), para(D, L, B, C)\nP = meet(circle(B, D, L), circumcircle(A, B, C))\nO1 = circumcenter(B, D, L)\nX = point: coll(B, S, X), perp(X, P, P, O1)\nprove eqangle(A, B, A, X, A, X, A, C)" },
    { id: "false", en: "A false claim (median ⟂ side)", ro: "O afirmație falsă (mediana ⟂ latura)", tag: "false",
      c: { en: "A false claim: the median from C is perpendicular to AB.", ro: "O afirmație falsă: mediana din C este perpendiculară pe AB." },
      body: "A B C = triangle\nM = midpoint(A, B)\nprove perp(C, M, A, B)" },
    { id: "numeric", en: "True, but only numerically", ro: "Adevărat, dar doar numeric", tag: "numeric",
      c: { en: "The median splits the triangle into two triangles of equal area.", ro: "Mediana împarte triunghiul în două triunghiuri de arii egale." },
      body: "A B C = triangle\nM = midpoint(B, C)\nprove area(A,B,M) = area(A,M,C)" },
  ];
  function exampleSrc(ex, lang) { return ex.c[lang || window.i18n.current()].split("\n").map(function (l) { return "# " + l + "\n"; }).join("") + ex.body; }
  function exampleOf(src) {
    for (var i = 0; i < EXAMPLES.length; i++) {
      if (src === exampleSrc(EXAMPLES[i], "en") || src === exampleSrc(EXAMPLES[i], "ro")) return EXAMPLES[i];
    }
    return null;
  }

  var S = {
    status: null, mode: "geo", effort: "standard", photo: null,
    busy: false, abort: null, timer: null, started: 0, deadline: 60,
    sol: null, geo: "", title: null, steps: null,
    aiCache: {}, aiSeq: 0, history: [], filter: "all", query: "", pendingDeletes: new Map(),
    refining: null, activeHistory: null, stage: null, lastError: null, skew: 0, stalled: false,
  };
  var STALL_GRACE = 35;

  function paintIcons(root) {
    (root || document).querySelectorAll("[data-icon]").forEach(function (el) {
      if (!el.firstChild) el.innerHTML = icons[el.getAttribute("data-icon")] || "";
    });
  }
  function setIcon(id, name) { var e = $(id); if (e) e.innerHTML = icons[name]; }
  var STATUSES = ["proved", "holds-numerically", "refuted", "not-proved"];
  function validSolution(d) { return !!d && typeof d.input === "string" && STATUSES.indexOf(d.status) >= 0; }
  function unreadable() { return { titleKey: "err.title.unreadable", bodyKey: "err.body.unreadable", retry: true }; }
  function undoKeys() { return isMac ? "⌘Z" : "Ctrl+Z"; }
  function placeMenu(menu, alignRight) {
    var wrap = menu.parentElement, vw = document.documentElement.clientWidth;
    menu.style.right = "auto";
    menu.style.left = "-10000px";
    menu.hidden = false;
    var w = menu.getBoundingClientRect().width, wr = wrap.getBoundingClientRect();
    var x = wr.left + (alignRight ? wr.width - w : 0);
    x = Math.max(8, Math.min(x, vw - 8 - w));
    menu.style.left = Math.round(x - wr.left) + "px";
  }

  // ---------------------------------------------------------------- status --
  function api(url, opts) {
    opts = opts || {};
    var init = { method: opts.method || "GET", headers: {}, signal: opts.signal, keepalive: !!opts.keepalive };
    if (opts.body !== undefined) { init.headers["Content-Type"] = "application/json"; init.body = JSON.stringify(opts.body); }
    return fetch(url, init).then(function (res) {
      var served = Date.parse(res.headers.get("date") || "");
      if (served) S.skew = served - Date.now();
      var ct = res.headers.get("content-type") || "";
      if (opts.raw && res.ok) return { ok: true, status: res.status, res: res };
      var sig = res.headers.get("x-solution-sig");
      return res.text().then(function (x) {
        if (ct.indexOf("json") >= 0) {
          var d = JSON.parse(x);
          if (sig && d && typeof d === "object") Object.defineProperty(d, "signed", { value: { body: x, sig: sig }, writable: true, configurable: true });
          return d;
        }
        x = (x || "").trim();
        return /^[^<]{1,200}$/.test(x) && ct.indexOf("html") < 0 ? { detail: x } : {};
      })
        .catch(function () { return {}; })
        .then(function (data) {
          if (GS.isGate(res.status, data)) return GS.toGate();
          if (res.ok && S.sessionEnded && url !== "/api/status") recheckSession();
          return { ok: res.ok, status: res.status, data: data || {}, retryAfter: parseInt(res.headers.get("retry-after"), 10) || null };
        });
    });
  }

  var recheckAt = 0;
  function recheckSession() {
    if (!S.sessionEnded || Date.now() - recheckAt < 1500) return;
    recheckAt = Date.now();
    api("/api/status").then(function (r) {
      if (!r.ok || !r.data.signed_in || !S.sessionEnded) return;
      S.sessionEnded = false;
      S.status = r.data;
      S.deadline = r.data.solve_deadline_secs || S.deadline;
      paintAccount();
      paintAiPill();
      paintGates();
      $("hist-empty").textContent = t("hist.loading");
      loadHistory();
    }).catch(function () {});
  }
  document.addEventListener("visibilitychange", function () { if (!document.hidden) recheckSession(); });
  window.addEventListener("focus", recheckSession);
  function coarse() { return matchMedia("(pointer: coarse)").matches; }

  window.addEventListener("pageshow", function (e) {
    if (!e.persisted) return;
    api("/api/status").then(function (r) {
      if (r.ok && !r.data.signed_in && !r.data.guest) location.replace("/auth");
    });
  });

  function loadStatus(attempt) {
    attempt = attempt || 0;
    api("/api/status").then(function (r) {
      if (!r.ok) throw new Error("status");
      S.status = r.data;
      S.deadline = r.data.solve_deadline_secs || 60;
      paintAccount();
      paintAiPill();
      paintGates();
      paintEffortHint();
      firstModeChoice();
      if (r.data.translate_checking && attempt < 20) setTimeout(function () { loadStatus(attempt + 1); }, 1500);
      if (attempt === 0 && r.data.signed_in) {
        loadHistory();
        var pending = store.get("gs.guest.resolve");
        store.del("gs.guest.resolve");
        if (pending && pending === editor.get().trim() && !S.busy && !S.sol) { setMode("geo"); solve(); }
      }
    }).catch(function () {
      if (attempt < 6) setTimeout(function () { loadStatus(attempt + 1); }, Math.min(30000, 2000 * Math.pow(2, attempt)));
    });
  }
  window.addEventListener("online", function () {
    if (!S.status) loadStatus(1);
    var e = S.lastError;
    if (e && e.offline && !S.busy && !$("state-error").hidden) { S.autoRetrying = true; solve(); }
  });

  function paintAccount() {
    var st = S.status || {};
    $("forget-device").hidden = st.gate !== "cookie";
    $("account").hidden = !st.signed_in;
    $("signin").hidden = !!st.signed_in || !(st.guest || S.sessionEnded);
    if (st.signed_in) {
      $("username").textContent = st.username;
      $("avatar").textContent = (st.username || "?").slice(0, 1);
      $("account").setAttribute("title", t("app.signed_in_as", { name: st.username }));
    }
    var rail = !!st.signed_in || !!S.sessionEnded;
    document.body.classList.toggle("has-rail", rail);
    $("rail").hidden = !rail;
    $("rail-toggle").hidden = !rail;
    var hdr = $("rail-toggle").closest(".site-header");
    hdr.classList.toggle("has-signin", !$("signin").hidden);
    hdr.classList.toggle("has-rail-toggle", rail);
  }

  function paintAiPill() {
    var st = S.status;
    var cls = "checking", key = "ai.checking", tip = t("ai.reason.checking");
    if (st && !st.translate_checking) {
      if (st.can_translate) { cls = "on"; key = "ai.on"; tip = t("ai.tip.on"); }
      else if (st.translate_block === "sign_in") { cls = "off"; key = "ai.signin"; tip = t("ai.reason.sign_in"); }
      else { cls = "off"; key = aiOffKey(st.translate_block); tip = t("ai.reason." + (st.translate_block || "disabled")); }
    }
    ["ai-pill", "ai-pill-m"].forEach(function (id) {
      var pill = $(id), txt = pill.querySelector(".pill-text");
      pill.className = "status-pill " + cls;
      txt.textContent = t(key);
      txt.setAttribute("data-i18n", key);
      pill.title = tip;
    });
  }

  function aiOffKey(block) {
    if (block === "sign_in") return "ai.signin";
    return block === "not_installed" || block === "not_logged_in" ? "ai.unset" : "ai.off";
  }

  function aiBlock() {
    var st = S.status;
    if (!st) return "checking";
    if (st.can_translate) return null;
    return st.translate_block || "disabled";
  }

  function paintGates() {
    var block = aiBlock();
    var emptyBody = document.querySelector("[data-i18n^='empty.body']");
    if (emptyBody) {
      var ek = block && block !== "checking" ? "empty.body.geo" : "empty.body";
      emptyBody.setAttribute("data-i18n", ek);
      emptyBody.textContent = t(ek);
    }
    var off = block && block !== "checking";
    ["tab-describe", "tab-photo"].forEach(function (id) {
      var tab = $(id), m = tab.querySelector(".tab-ai");
      if (!m) { m = document.createElement("span"); m.className = "tab-ai"; tab.appendChild(m); }
      m.innerHTML = off ? '<span class="tab-ai-dot" aria-hidden="true"></span><span class="sr-only">' + esc(t(aiOffKey(block))) + "</span>" : "";
      m.hidden = !off;
    });
    document.querySelectorAll("[data-gate]").forEach(function (g) {
      var body = g.parentNode.querySelector(".ai-body");
      if (!block) { g.hidden = true; body.hidden = false; return; }
      g.hidden = false;
      body.hidden = true;
      var actions = '<button type="button" class="btn btn-secondary btn-sm" data-goto-geo>' + esc(t("unavailable.write_geo")) + "</button>";
      if (block === "sign_in") actions = '<a class="btn btn-primary btn-sm" href="/auth">' + esc(t("unavailable.signin")) + '</a><a class="btn btn-secondary btn-sm" href="/auth?mode=register">' + esc(t("nav.create")) + "</a>" + actions;
      g.innerHTML = '<div class="banner ' + (block === "checking" ? "tone-info" : "tone-neutral") + '">' + (block === "checking" ? '<span class="spinner" aria-hidden="true"></span>' : icons.info) +
        "<div><p>" + esc(t("ai.reason." + block)) + '</p><div class="gate-actions">' + actions + "</div></div></div>";
    });
    paintSolveEnabled();
  }

  // ------------------------------------------------------------------ tabs --
  var MODES = ["describe", "photo", "geo"];
  function setMode(m, focus, picked) {
    S.mode = m;
    if (picked !== "boot") S.modePicked = true;
    if (picked === true) store.set("gs.mode", m);
    MODES.forEach(function (x) {
      var tab = $("tab-" + x), on = x === m;
      tab.setAttribute("aria-selected", on ? "true" : "false");
      tab.tabIndex = on ? 0 : -1;
      $("panel-" + x).hidden = !on;
    });
    if (focus) $("tab-" + m).focus();
    paintSolveEnabled();
    if (m === "geo") editor.refresh();
  }
  function firstModeChoice() {
    if (S.modePicked || aiBlock() === "checking") return;
    S.modePicked = true;
    var a = document.activeElement;
    if (a && a !== document.body && a.closest && a.closest("#composer")) return;
    if (aiBlock()) { setMode("geo"); return; }
    var saved = store.get("gs.mode");
    setMode(MODES.indexOf(saved) >= 0 ? saved : "describe");
  }
  function wireTabs() {
    MODES.forEach(function (m, i) {
      var tab = $("tab-" + m);
      tab.addEventListener("click", function () { setMode(m, false, true); });
      tab.addEventListener("keydown", function (e) {
        var j = null;
        if (e.key === "ArrowRight") j = (i + 1) % MODES.length;
        else if (e.key === "ArrowLeft") j = (i + MODES.length - 1) % MODES.length;
        else if (e.key === "Home") j = 0;
        else if (e.key === "End") j = MODES.length - 1;
        if (j != null) { e.preventDefault(); setMode(MODES[j], true, true); }
      });
    });
    document.addEventListener("click", function (e) {
      if (e.target.closest("[data-goto-geo]")) { setMode("geo"); $("geo-input").focus(); }
    });
  }

  // ---------------------------------------------------------------- editor --
  var KEYWORDS = { prove: 1, goal: 1, point: 1 };
  var FUNCS = "triangle segment line circle circumcircle midpoint circumcenter circumcentre orthocenter orthocentre incenter incentre centroid foot reflect mirror parallelogram meet intersect bisector perp_bisector perp_line para_line tangent eq_triangle square on_dia on_line on_circle on_circum on_bline on_pline on_tline shift excenter nine_point_center ninepoints iso_triangle free coll cyclic cong perp para eqangle eqratio on dist angle area sqrt sin cos".split(" ");
  var FUNCSET = {}; FUNCS.forEach(function (f) { FUNCSET[f] = 1; });
  function highlightLine(line) {
    var out = "", i = 0;
    var hash = line.indexOf("#");
    var code = hash >= 0 ? line.slice(0, hash) : line;
    var re = /([A-Za-z_][A-Za-z0-9_']*)|(\d+(?:\.\d+)?)|(\s+)|([(),:=;?^*+\-/])|(.)/g, m;
    while ((m = re.exec(code))) {
      if (m[1]) {
        var w = m[1];
        if (KEYWORDS[w]) out += '<span class="tk-kw">' + w + "</span>";
        else if (FUNCSET[w]) out += '<span class="tk-fn">' + w + "</span>";
        else if (/^[A-Z]/.test(w)) out += '<span class="tk-pt">' + esc(w) + "</span>";
        else out += '<span class="tk-id">' + esc(w) + "</span>";
      } else if (m[2]) out += '<span class="tk-num">' + m[2] + "</span>";
      else if (m[3]) out += m[3];
      else if (m[4]) out += '<span class="tk-p">' + esc(m[4]) + "</span>";
      else out += esc(m[5]);
    }
    if (hash >= 0) out += '<span class="tk-com">' + esc(line.slice(hash)) + "</span>";
    void i;
    return out;
  }
  var editor = (function () {
    var ta = $("geo-input"), code = $("hl-code"), gutter = $("gutter"), pre = $("hl");
    var err = null, saveT = null;
    function render() {
      var lines = ta.value.split("\n");
      var html = lines.map(function (ln, idx) {
        var h;
        if (err && err.line === idx + 1) {
          var c0 = Math.max(0, err.col - 1), c1 = Math.min(ln.length, c0 + Math.max(1, err.len || 1));
          if (c0 >= ln.length) h = highlightLine(ln) + '<span class="tk-err tk-eol"> </span>';
          else h = highlightLine(ln.slice(0, c0)) + '<span class="tk-err">' + esc(ln.slice(c0, c1)) + "</span>" + highlightLine(ln.slice(c1));
        } else h = highlightLine(ln);
        return h;
      }).join("\n") + "\n";
      code.innerHTML = html;
      gutter.innerHTML = lines.map(function (_, idx) {
        return '<span' + (err && err.line === idx + 1 ? ' class="is-err"' : "") + ">" + (idx + 1) + "</span>";
      }).join("");
      var rows = Math.min(18, Math.max(6, lines.length + 1));
      ta.style.height = rows * (parseFloat(getComputedStyle(ta).lineHeight) || 22.4) + 24 + "px";
      sync();
    }
    function sync() {
      pre.scrollTop = ta.scrollTop;
      pre.scrollLeft = ta.scrollLeft;
      gutter.scrollTop = ta.scrollTop;
      ta.parentNode.classList.toggle("can-scroll-x", ta.scrollWidth - ta.clientWidth - ta.scrollLeft > 2);
    }
    if (window.ResizeObserver) new ResizeObserver(function () { sync(); }).observe(ta);
    ta.addEventListener("input", function () {
      if (err) setError(null);
      render();
      clearTimeout(saveT);
      saveT = setTimeout(function () { store.set("gs.draft.geo", ta.value); }, 400);
      paintSolveEnabled();
    });
    ta.addEventListener("scroll", sync);
    function setError(e) {
      err = e && e.line ? e : null;
      ta.setAttribute("aria-invalid", err ? "true" : "false");
      var box = $("geo-error");
      if (e && e.message) {
        box.hidden = false;
        box.innerHTML = icons.alert + "<span>" + (err ? '<strong>' + esc(t("err.where", { line: err.line, col: err.col })) + "</strong> · " : "") + esc(e.message) + "</span>";
      } else { box.hidden = true; box.innerHTML = ""; }
      render();
    }
    function focusError() {
      if (!err) return;
      var lines = ta.value.split("\n"), pos = 0;
      for (var i = 0; i < err.line - 1 && i < lines.length; i++) pos += lines[i].length + 1;
      pos += Math.max(0, err.col - 1);
      ta.focus();
      ta.setSelectionRange(pos, pos + Math.max(1, err.len || 1));
    }
    return {
      get: function () { return ta.value; },
      set: function (v) { ta.value = v; setError(null); render(); store.set("gs.draft.geo", v); paintSolveEnabled(); },
      refresh: render, setError: setError, focusError: focusError,
    };
  })();

  // -------------------------------------------------------------- examples --
  function exampleName(ex) { return window.i18n.current() === "ro" ? ex.ro : ex.en; }
  function loadExample(id, solveNow) {
    var ex = EXAMPLES.filter(function (x) { return x.id === id; })[0];
    if (!ex) return;
    setMode("geo");
    editor.set(exampleSrc(ex));
    S.baseline = exampleSrc(ex);
    closeMenu();
    if (solveNow) solve();
    else $("geo-input").focus();
  }
  function paintExamples() {
    $("examples-menu").innerHTML = EXAMPLES.map(function (ex) {
      return '<button type="button" role="menuitem" data-example="' + ex.id + '"><span>' + esc(exampleName(ex)) + '</span><span class="menu-sub" aria-hidden="true">' + esc(t("tag." + ex.tag)) + "</span></button>";
    }).join("");
    $("example-chips").innerHTML = EXAMPLES.filter(function (x) { return ["ortho", "euler", "stewart", "imo2023", "false", "numeric"].indexOf(x.id) >= 0; }).map(function (ex) {
      return '<button type="button" class="btn btn-secondary btn-sm" data-example-solve="' + ex.id + '">' + esc(exampleName(ex)) + "</button>";
    }).join("");
  }
  var menuOpen = false;
  function openMenu() {
    var m = $("examples-menu");
    placeMenu(m, true); menuOpen = true;
    $("examples-btn").setAttribute("aria-expanded", "true");
    var first = m.querySelector("[role=menuitem]");
    if (first) first.focus();
  }
  function closeMenu(focusBtn) {
    if (!menuOpen) return;
    $("examples-menu").hidden = true; menuOpen = false;
    $("examples-btn").setAttribute("aria-expanded", "false");
    if (focusBtn) $("examples-btn").focus();
  }
  function wireExamples() {
    $("examples-btn").addEventListener("click", function () { menuOpen ? closeMenu() : openMenu(); });
    $("examples-menu").addEventListener("click", function (e) {
      var b = e.target.closest("[data-example]");
      if (b) loadExample(b.getAttribute("data-example"));
    });
    $("examples-menu").addEventListener("keydown", function (e) {
      var items = Array.prototype.slice.call($("examples-menu").querySelectorAll("[role=menuitem]"));
      var i = items.indexOf(document.activeElement);
      if (e.key === "ArrowDown") { e.preventDefault(); items[(i + 1) % items.length].focus(); }
      else if (e.key === "ArrowUp") { e.preventDefault(); items[(i - 1 + items.length) % items.length].focus(); }
      else if (e.key === "Home") { e.preventDefault(); items[0].focus(); }
      else if (e.key === "End") { e.preventDefault(); items[items.length - 1].focus(); }
      else if (e.key === "Escape" || e.key === "Tab") { if (e.key === "Escape") e.preventDefault(); closeMenu(e.key === "Escape"); }
    });
    document.addEventListener("click", function (e) {
      if (menuOpen && !e.target.closest(".menu-wrap")) closeMenu();
      var s = e.target.closest("[data-example-solve]");
      if (s) loadExample(s.getAttribute("data-example-solve"), true);
    });
    $("syntax-btn").addEventListener("click", function () {
      var open = $("syntax-help").hidden;
      $("syntax-help").hidden = !open;
      $("syntax-btn").setAttribute("aria-expanded", open ? "true" : "false");
    });
  }

  // ---------------------------------------------------------------- effort --
  function setEffort(v, focus) {
    S.effort = v;
    store.set("gs.effort", v);
    document.querySelectorAll("#effort [role=radio]").forEach(function (b) {
      var on = b.getAttribute("data-effort") === v;
      b.setAttribute("aria-checked", on ? "true" : "false");
      b.tabIndex = on ? 0 : -1;
      if (on && focus) b.focus();
    });
    paintEffortHint();
  }
  function paintEffortHint() {
    $("effort-hint").textContent = S.effort === "shortest" ? t("effort.hint.shortest") : t("effort.hint.standard", { s: S.deadline });
  }
  function wireEffort() {
    var btns = Array.prototype.slice.call(document.querySelectorAll("#effort [role=radio]"));
    btns.forEach(function (b, i) {
      b.addEventListener("click", function () { setEffort(b.getAttribute("data-effort")); });
      b.addEventListener("keydown", function (e) {
        if (["ArrowRight", "ArrowDown", "ArrowLeft", "ArrowUp"].indexOf(e.key) < 0) return;
        e.preventDefault();
        var j = (e.key === "ArrowRight" || e.key === "ArrowDown") ? (i + 1) % btns.length : (i + btns.length - 1) % btns.length;
        setEffort(btns[j].getAttribute("data-effort"), true);
      });
    });
    setEffort(store.get("gs.effort") === "shortest" ? "shortest" : "standard");
  }

  // ----------------------------------------------------------------- photo --
  var PHOTO_STEPS = [[2048, 0.85], [1600, 0.8], [1280, 0.75]];
  var PHOTO_RAW_MAX = 80 * 1024 * 1024;
  function photoLimit() { return (S.status && S.status.max_image_bytes) || 6 * 1024 * 1024; }
  function isHeic(file) { return /image\/hei[cf]/i.test(file.type || "") || /\.hei[cf]$/i.test(file.name || ""); }
  function isImageFile(file) { return /^image\//i.test(file.type || "") || isHeic(file) || (!file.type && /\.(jpe?g|png|webp|gif|bmp|avif|tiff?)$/i.test(file.name || "")); }
  function fmtBytes(n) {
    var lang = window.i18n.current();
    if (n < 1024 * 1024) return Math.max(1, Math.round(n / 1024)).toLocaleString(lang) + " KB";
    return (n / 1024 / 1024).toLocaleString(lang, { maximumFractionDigits: 1 }) + " MB";
  }
  function loadImage(file) {
    return new Promise(function (resolve, reject) {
      var url = URL.createObjectURL(file);
      var img = new Image();
      img.onload = function () {
        if (!img.naturalWidth || !img.naturalHeight) { URL.revokeObjectURL(url); reject(new Error("empty")); return; }
        resolve({ img: img, url: url });
      };
      img.onerror = function () { URL.revokeObjectURL(url); reject(new Error("decode")); };
      img.src = url;
    });
  }
  function canvasOf(w, h) {
    var c = document.createElement("canvas");
    c.width = w; c.height = h;
    return c;
  }
  function drawScaled(img, edge) {
    var w = img.naturalWidth || img.width, h = img.naturalHeight || img.height;
    var k = Math.min(1, edge / Math.max(w, h));
    var tw = Math.max(1, Math.round(w * k)), th = Math.max(1, Math.round(h * k));
    var src = img, sw = w, sh = h, scratch = [];
    while (sw / 2 >= tw * 1.05 && sh / 2 >= th * 1.05) {
      var half = canvasOf(Math.round(sw / 2), Math.round(sh / 2));
      var hg = half.getContext("2d");
      hg.imageSmoothingEnabled = true; hg.imageSmoothingQuality = "high";
      hg.drawImage(src, 0, 0, half.width, half.height);
      scratch.push(half);
      src = half; sw = half.width; sh = half.height;
    }
    var out = canvasOf(tw, th), g = out.getContext("2d");
    g.fillStyle = "#fff";
    g.fillRect(0, 0, tw, th);
    g.imageSmoothingEnabled = true; g.imageSmoothingQuality = "high";
    g.drawImage(src, 0, 0, tw, th);
    scratch.forEach(function (c) { c.width = 0; c.height = 0; });
    return out;
  }
  function canvasJpeg(canvas, quality) {
    return new Promise(function (resolve, reject) {
      if (canvas.toBlob) {
        canvas.toBlob(function (b) { b && b.size ? resolve(b) : reject(new Error("encode")); }, "image/jpeg", quality);
        return;
      }
      try {
        var bin = atob(canvas.toDataURL("image/jpeg", quality).split(",")[1]), arr = new Uint8Array(bin.length);
        for (var i = 0; i < bin.length; i++) arr[i] = bin.charCodeAt(i);
        resolve(new Blob([arr], { type: "image/jpeg" }));
      } catch (e) { reject(e); }
    });
  }
  function blobDataUrl(blob) {
    return new Promise(function (resolve, reject) {
      var r = new FileReader();
      r.onload = function () { resolve(r.result); };
      r.onerror = function () { reject(r.error || new Error("read")); };
      r.readAsDataURL(blob);
    });
  }
  function normalizePhoto(file) {
    var limit = photoLimit();
    return loadImage(file).catch(function () {
      throw { key: isHeic(file) ? "photo.err.heic" : "photo.err.unreadable" };
    }).then(function (loaded) {
      var attempt = function (i) {
        var canvas = drawScaled(loaded.img, PHOTO_STEPS[i][0]);
        var w = canvas.width, h = canvas.height;
        return canvasJpeg(canvas, PHOTO_STEPS[i][1]).then(function (blob) {
          canvas.width = 0; canvas.height = 0;
          if (blob.size <= limit) return { blob: blob, w: w, h: h };
          if (i + 1 < PHOTO_STEPS.length) return attempt(i + 1);
          throw { key: "photo.err.too_large" };
        });
      };
      return attempt(0).then(function (r) { URL.revokeObjectURL(loaded.url); return r; }, function (e) {
        URL.revokeObjectURL(loaded.url);
        throw e && e.key ? e : { key: "photo.err.unreadable" };
      });
    }).then(function (r) {
      return blobDataUrl(r.blob).then(function (b64) {
        return { b64: b64, bytes: r.blob.size, w: r.w, h: r.h, blob: r.blob, name: file.name || "photo.jpg" };
      });
    });
  }
  function paintPhoto(p) {
    var prev = $("photo-preview");
    if (S.photoUrl) { URL.revokeObjectURL(S.photoUrl); S.photoUrl = null; }
    $("dz-busy").hidden = true;
    if (!p) {
      prev.removeAttribute("src");
      $("dz-empty").hidden = false; $("dz-full").hidden = true; $("dz-actions").hidden = true;
      return;
    }
    S.photoUrl = URL.createObjectURL(p.blob);
    prev.src = S.photoUrl;
    prev.alt = t("photo.alt");
    $("photo-name").textContent = p.name;
    $("photo-meta").textContent = t("photo.meta", { w: p.w, h: p.h, size: fmtBytes(p.bytes) });
    $("dz-empty").hidden = true; $("dz-full").hidden = false; $("dz-actions").hidden = false;
  }
  function setPhoto(file) {
    var seq = (S.photoSeq || 0) + 1;
    S.photoSeq = seq;
    S.photoErrKey = null;
    fieldError("photo-err", null);
    if (!file) {
      S.photo = null; S.photoPending = null;
      paintPhoto(null);
      $("photo-input").value = "";
      paintSolveEnabled();
      return;
    }
    var fail = function (key) {
      if (S.photoSeq !== seq) return;
      S.photo = null; S.photoPending = null;
      paintPhoto(null);
      $("photo-input").value = "";
      S.photoErrKey = key;
      fieldError("photo-err", t(key));
      paintSolveEnabled();
    };
    if (!isImageFile(file)) { fail("photo.err.type"); return; }
    if (file.size > PHOTO_RAW_MAX) { fail("photo.err.too_large"); return; }
    S.photo = null;
    $("dz-empty").hidden = true; $("dz-full").hidden = true; $("dz-busy").hidden = false;
    $("photo-status").textContent = t("photo.preparing");
    S.photoPending = normalizePhoto(file).then(function (p) {
      if (S.photoSeq !== seq) return;
      S.photo = p; S.photoPending = null;
      paintPhoto(p);
      $("photo-status").textContent = p.name + " · " + $("photo-meta").textContent;
      paintSolveEnabled();
    }, function (e) { fail((e && e.key) || "photo.err.unreadable"); });
  }
  function wirePhoto() {
    var dz = $("dropzone");
    $("photo-input").addEventListener("change", function (e) { if (e.target.files[0]) setPhoto(e.target.files[0]); });
    $("photo-input").addEventListener("focus", function () { $("dropzone").classList.add("is-focused"); });
    $("photo-input").addEventListener("blur", function () { $("dropzone").classList.remove("is-focused"); });
    dz.addEventListener("dragover", function (e) { e.preventDefault(); dz.classList.add("is-over"); });
    dz.addEventListener("dragleave", function () { dz.classList.remove("is-over"); });
    dz.addEventListener("drop", function (e) { e.preventDefault(); dz.classList.remove("is-over"); if (e.dataTransfer.files[0]) setPhoto(e.dataTransfer.files[0]); });
    $("photo-remove").addEventListener("click", function () { setPhoto(null); $("photo-input").focus(); });
    if (!coarse()) { var dropTxt = dz.querySelector('[data-i18n="photo.drop"]'); if (dropTxt) { dropTxt.setAttribute("data-i18n", "photo.drop.paste"); dropTxt.textContent = t("photo.drop.paste"); } }
    var hasFiles = function (e) { return e.dataTransfer && Array.prototype.indexOf.call(e.dataTransfer.types || [], "Files") >= 0; };
    document.addEventListener("dragover", function (e) { if (hasFiles(e)) { e.preventDefault(); e.dataTransfer.dropEffect = aiBlock() || S.busy ? "none" : "copy"; } });
    document.addEventListener("drop", function (e) {
      if (!hasFiles(e) || dz.contains(e.target)) return;
      e.preventDefault();
      takeImage(e.dataTransfer.files);
    });
    document.addEventListener("paste", function (e) {
      var dt = e.clipboardData;
      if (!dt || !dt.files || !dt.files.length) return;
      var inText = e.target.closest && e.target.closest("textarea, input");
      if (inText && (dt.getData("text/plain") || "").trim()) return;
      if (takeImage(dt.files)) e.preventDefault();
    });
  }
  function takeImage(files) {
    var f = Array.prototype.filter.call(files || [], isImageFile)[0];
    if (!f || aiBlock() || S.busy) return false;
    if (S.mode !== "photo") setMode("photo", false, true);
    setPhoto(f);
    return true;
  }

  // ----------------------------------------------------------------- solve --
  function paintSolveEnabled() {
    var ok = !S.busy;
    if (S.mode !== "geo" && aiBlock()) ok = false;
    $("solve").disabled = !ok;
    $("clear").hidden = S.busy || (S.mode !== "geo" && !!aiBlock());
  }
  function clearable() {
    if (S.mode === "geo") return !!editor.get();
    if (aiBlock()) return false;
    if (S.mode === "describe") return !!$("describe-input").value;
    return !!S.photo;
  }

  function show(which) {
    clearInterval(S.retryTimer);
    S.retryTimer = null;
    ["state-empty", "state-solving", "state-error", "verdict"].forEach(function (id) { $(id).hidden = id !== which; });
    paintDocTitle();
  }
  function paintDocTitle() {
    var parts = [];
    if (!$("verdict").hidden && S.sol) {
      if (S.sol.title) parts.push(S.sol.title);
      parts.push(verdictModel(S.sol).head);
    } else if (!$("state-error").hidden && S.lastError) parts.push(S.lastError.titleKey ? t(S.lastError.titleKey) : S.lastError.title);
    else if (!$("state-solving").hidden) parts.push($("solving-title").textContent || t("solving.solving"));
    document.title = parts.length ? parts.join(" — ") + " · GeoSolver" : "GeoSolver";
  }

  function stagesFor(mode) {
    return mode === "geo" ? ["solve"] : ["translate", "solve"];
  }
  function paintStepper(stages, current) {
    $("stepper").innerHTML = stages.map(function (s, i) {
      var cls = i < current ? "done" : i === current ? "current" : "";
      return '<li class="' + cls + '"' + (i === current ? ' aria-current="step"' : "") + "><span class=\"st-dot\" aria-hidden=\"true\"></span>" + esc(t("stage." + s)) + "</li>";
    }).join("");
    $("stepper").hidden = stages.length < 2;
  }

  function translateLimit() { return (S.status && S.status.translate_timeout_secs) || 90; }
  function startTimer(maxSecs) {
    S.started = Date.now();
    clearInterval(S.timer);
    var tick = function () {
      var el = (Date.now() - S.started) / 1000;
      $("solving-elapsed").textContent = el > maxSecs
        ? t("solving.finishing", { max: maxSecs + "\u00a0s" })
        : t("solving.elapsed", { t: Math.floor(el) + "\u00a0s", max: maxSecs + "\u00a0s" });
      $("progress-bar").style.width = Math.min(100, (el / maxSecs) * 100) + "%";
    };
    tick();
    S.timer = setInterval(tick, 250);
  }
  function stopTimer() { clearInterval(S.timer); S.timer = null; S.queued = false; }

  function setBusy(on) {
    S.busy = on;
    $("cancel").hidden = true;
    paintSolveEnabled();
  }

  function clearResult() {
    S.sol = null; S.steps = null;
    $("proof-area").hidden = true;
    viewer.setSvg("");
    $("fig-empty").hidden = false;
    $("fig-legend").hidden = true;
    figTools(false);
    document.body.classList.remove("has-result");
  }
  function figTools(on) {
    ["z-out", "z-in", "z-fit", "z-full", "z-svg"].forEach(function (id) { $(id).disabled = !on; });
    $("fig-hint").hidden = !on;
    var vp = $("fig-viewport");
    vp.style.height = "";
    vp.tabIndex = on ? 0 : -1;
    if (on) vp.removeAttribute("aria-labelledby"); else vp.setAttribute("aria-labelledby", "fig-empty");
    $("fig-frame").querySelector(".fig-foot").hidden = !on;
  }
  var figHintNow = "";
  function paintFigHint() {
    var coarse = matchMedia("(pointer: coarse)").matches;
    var wheel = !coarse && typeof viewer !== "undefined" && viewer && !viewer.pinned();
    var html = coarse ? esc(t("fig.hint.coarse"))
      : esc(t(wheel ? "fig.hint.wheel" : "fig.hint.fine", { key: "\u0001", mod: isMac ? "⌘" : "Ctrl" })).replace("\u0001", "<kbd>0</kbd>");
    if (html !== figHintNow) { figHintNow = html; $("fig-hint").innerHTML = html; }
  }
  function fieldError(id, msg) {
    var el = $(id);
    if (!el) return;
    S.lastFieldErr = msg ? id : (S.lastFieldErr === id ? null : S.lastFieldErr);
    el.hidden = !msg;
    el.innerHTML = msg ? icons.alert + "<span>" + esc(msg) + "</span>" : "";
  }

  function startQueued(waitSecs, then) {
    S.queued = true;
    paintStage();
    S.started = Date.now();
    clearInterval(S.timer);
    var tick = function () {
      var el = (Date.now() - S.started) / 1000;
      if (el >= waitSecs) { S.queued = false; paintStage(); then(); return; }
      $("solving-elapsed").textContent = t("solving.queued.x", { t: Math.floor(el) + "\u00a0s", max: Math.round(waitSecs) + "\u00a0s" });
      $("progress-bar").style.width = "0%";
    };
    tick();
    S.timer = setInterval(tick, 250);
  }

  function solve() {
    if (S.busy) return;
    S.autoRetried = !!S.autoRetrying;
    S.autoRetrying = false;
    S.queued = false;
    var stoppedRefine = !!S.refining;
    if (S.refining) stopRefining();
    var mode = S.mode;
    var geo = editor.get().trim();
    var describe = $("describe-input").value.trim();
    if (mode === "geo" && !geo) { editor.setError({ message: t("err.empty_geo") }); $("geo-input").focus(); return; }
    fieldError("describe-err", null); fieldError("photo-err", null);
    if (mode === "describe" && !describe) { fieldError("describe-err", t("err.empty_describe")); $("describe-input").focus(); return; }
    if (mode === "photo" && !S.photo && S.photoPending) {
      var waitFor = S.photoPending;
      waitFor.then(function () { if (S.photoPending === null && S.photo && S.mode === "photo" && !S.busy) solve(); });
      return;
    }
    if (mode === "photo" && !S.photo) { S.photoErrKey = null; fieldError("photo-err", t("err.empty_photo")); $("photo-input").focus(); return; }
    if (mode !== "geo" && aiBlock()) return;
    editor.setError(null);
    S.lastError = null;
    if (S.cancelToast) { S.cancelToast.close(); S.cancelToast = null; }
    if (S.activeHistory != null) { S.activeHistory = null; renderHistory(); }
    var ae = document.activeElement;
    var moveFocus = !ae || ae === document.body || ae === $("solve") || !!(ae.closest && ae.closest("[data-example-solve], #state-error, #verdict"));
    clearResult();
    setBusy(true);
    var ctl = new AbortController();
    S.abort = ctl;
    var stages = stagesFor(mode);
    show("state-solving");
    if (moveFocus) $("cancel-2").focus({ preventScroll: true });
    var shortestFirst = S.effort === "shortest";
    var stall = null;
    function watch(secs) {
      clearTimeout(stall);
      stall = setTimeout(function () { if (S.abort === ctl) { S.stalled = true; ctl.abort(); } }, (secs + STALL_GRACE) * 1000);
    }
    function stage(i) {
      S.stage = { stages: stages, i: i };
      paintStage();
      var limit = stages[i] === "translate" ? translateLimit() : S.deadline;
      startTimer(limit);
      watch(limit);
    }
    stage(0);
    announce(t(mode === "geo" ? "solving.solving" : "solving.translating"));
    scrollToStatus();

    var title = null;
    var chain = Promise.resolve(geo);
    if (mode !== "geo") {
      var body = mode === "photo" ? { image_base64: S.photo.b64 } : { text: describe };
      chain = api("/api/translate", { method: "POST", body: body, signal: ctl.signal }).then(function (r) {
        if (!r.ok) throw httpError(r, "translate", mode);
        var g = r.data.geo || "";
        if (/cannot translate/i.test(g) || !g.trim()) throw { titleKey: "err.title.translate", bodyKey: "err.cannot_translate" };
        title = r.data.title || null;
        editor.set(g);
        S.baseline = g;
        stage(1);
        return g;
      });
    }
    chain.then(function (g) {
      S.geo = g;
      return api("/api/status", { signal: ctl.signal }).then(function (r) {
        if (!r.ok || r.data.solver_free !== false || ctl.signal.aborted) return g;
        clearTimeout(stall);
        startQueued(r.data.queue_wait_secs || 5, function () { startTimer(S.deadline); watch(S.deadline); });
        return g;
      }, function (e) {
        if (e && e.name === "AbortError") throw e;
        return g;
      });
    }).then(function (g) {
      var tries = stoppedRefine ? 4 : 0;
      var post = function () {
        return api("/api/solve", { method: "POST", body: { input: g, title: title }, signal: ctl.signal }).then(function (r) {
          if (r.status === 503 && r.data && r.data.code === "busy_self" && tries-- > 0) {
            return new Promise(function (res) { setTimeout(res, 350); }).then(function () {
              if (ctl.signal.aborted) throw new DOMException("aborted", "AbortError");
              return post();
            });
          }
          return r;
        });
      };
      return post().then(function (r) {
        if (!r.ok) throw httpError(r, "solve");
        if (!validSolution(r.data)) throw unreadable();
        return r.data;
      });
    }).then(function (sol) {
      clearTimeout(stall);
      stopTimer();
      setBusy(false);
      S.abort = null;
      renderSolution(sol, { announce: true, focus: true });
      if (S.status && S.status.signed_in) loadHistory();
      else if (S.status && S.status.guest) store.set("gs.guest.resolve", sol.input);
      if (shortestFirst && sol.status === "proved" && sol.method !== "euclidean" && stepCount(sol) > 1) refineShorter(sol);
    }).catch(function (e) {
      clearTimeout(stall);
      stopTimer();
      S.queued = false;
      var ae = document.activeElement;
      var lost = !ae || ae === document.body || $("state-solving").contains(ae);
      var quiet = S.quietCancel;
      S.quietCancel = false;
      setBusy(false);
      S.abort = null;
      if (S.stalled) {
        S.stalled = false;
        showError(navigator.onLine === false ? networkError(e) : { titleKey: "err.title.stalled", bodyKey: "err.body.stalled", retry: true }, lost);
        return;
      }
      if (e && e.name === "AbortError") {
        if (quiet) return;
        show("state-empty");
        S.cancelToast = GS.toast(t("cancelled"), { ms: 6000 });
        if (lost) $("solve").focus();
        return;
      }
      showError(e && (e.title || e.titleKey) ? e : networkError(e), lost);
    });
  }

  function cancel() {
    if (S.abort) S.abort.abort();
    if (S.refining) stopRefining();
  }
  function paintStage() {
    if (!S.stage) return;
    paintStepper(S.stage.stages, S.stage.i);
    $("solving-title").textContent = S.queued ? t("solving.queued") : S.stage.stages[S.stage.i] === "translate" ? t("solving.translating") : t("solving.solving");
    paintDocTitle();
  }

  function authLost() {
    if (S.status && S.status.signed_in) sessionEnded();
  }
  function sessionEnded() {
    if (S.sessionEnded) return;
    S.sessionEnded = true;
    S.history = [];
    S.activeHistory = null;
    var mark = histFocusMark();
    $("hist-list").innerHTML = "";
    $("hist-empty").hidden = false;
    $("hist-empty").innerHTML = esc(t("hist.session_ended")) + ' <a href="/auth">' + esc(t("nav.signin")) + "</a>";
    if (mark) histFocusRestore(mark);
    api("/api/status").then(function (r) {
      if (r.ok) { S.status = r.data; S.deadline = r.data.solve_deadline_secs || S.deadline; }
    }).catch(function () {}).then(function () {
      if (S.status) S.status.signed_in = false;
      paintAccount();
      paintAiPill();
      paintGates();
    });
  }
  function translateError(r, d, mode) {
    var photo = mode === "photo";
    if (r.status === 413 || d.code === "image_too_large") {
      return photo
        ? { titleKey: "err.title.photo_large", body: d.code === "image_too_large" ? d.error : null, bodyKey: d.code === "image_too_large" ? null : "err.body.photo_large" }
        : { titleKey: "err.title.describe_long", body: d.error && d.code !== "body_too_large" ? d.error : null, bodyKey: d.error && d.code !== "body_too_large" ? null : "err.body.describe_long" };
    }
    if (!d.error || r.status === 504) return null;
    var titleKey = d.code === "translate_timeout" ? "err.title.translate_timeout" : d.code === "heic" ? "err.title.photo_format" : "err.title.translate";
    return { titleKey: titleKey, body: d.error, retry: r.status >= 500 && d.code !== "translate_unavailable", detail: d.detail };
  }
  function httpError(r, what, mode) {
    var d = r.data || {};
    if (r.status === 400 && d.code === "compile") {
      return { titleKey: "err.title.compile", body: d.error, diagnosis: d.diagnosis, detail: d.detail, compile: true };
    }
    if (r.status === 401 && d.code === "sign_in") return { titleKey: "err.title.auth", body: d.error, bodyKey: d.error ? null : "err.body.auth", signin: true };
    if (r.status === 401) { authLost(); return { titleKey: "err.title.auth", bodyKey: "err.body.auth", signin: true }; }
    if (r.status === 429) return { titleKey: "err.title.rate", bodyKey: "err.body.rate", retry: true };
    if (r.status === 503 && d.code === "busy_self") return { titleKey: "err.title.busy_self", bodyKey: "err.body.busy_self", n: d.limit || 1, retry: true, retryAfter: r.retryAfter };
    if (r.status === 503 && (what !== "translate" || d.code === "busy")) return { titleKey: "err.title.busy", bodyKey: "err.body.busy", retry: true, retryAfter: r.retryAfter };
    if (what === "translate") {
      var te = translateError(r, d, mode);
      if (te) return te;
    }
    if (r.status === 502 || r.status === 503) return { titleKey: "err.title.unavailable", bodyKey: "err.body.unavailable", retry: true, detail: d.detail };
    if (r.status === 504) return { titleKey: "err.title.timeout", bodyKey: "err.body.timeout", retry: true };
    if (r.status === 413) return { titleKey: "err.title.too_large", body: d.error || "", bodyKey: d.error ? null : "err.body.too_large" };
    if (what === "translate") return { titleKey: "err.title.translate", body: d.error, bodyKey: d.error ? null : "err.cannot_translate", retry: r.status >= 500, detail: d.detail };
    if (d.error) return { titleKey: "err.title.generic", body: d.error, retry: true };
    return { titleKey: "err.title.generic", bodyKey: "err.body.http", vars: { status: r.status }, retry: true, detail: d.detail };
  }
  function errText(e, which) {
    var key = which === "title" ? e.titleKey : e.bodyKey;
    if (!key) return which === "title" ? e.title : e.body;
    return e.n != null ? tp(key, e.n, e.vars) : t(key, e.vars);
  }
  function networkError(e) {
    if (navigator.onLine === false) return { titleKey: "err.title.offline", bodyKey: "err.body.offline", retry: true, offline: true };
    return { titleKey: "err.title.network", bodyKey: "err.body.network", retry: true, detail: e && e.message };
  }

  function showError(e, focusHead) {
    S.lastError = e;
    if (e.titleKey) e.title = errText(e, "title");
    if (e.bodyKey) e.body = errText(e, "body");
    var box = $("state-error");
    var where = e.diagnosis && e.diagnosis.line ? '<p class="err-where"><button type="button" class="link-btn" id="err-goto">' + esc(t("err.where", { line: e.diagnosis.line, col: e.diagnosis.col })) + "</button></p>" : "";
    var actions = "";
    if (e.retry) actions += '<button type="button" class="btn btn-secondary btn-sm" id="err-retry">' + icons.retry + "<span>" + esc(t("err.retry")) + "</span></button>";
    var autoRetry = e.retry && e.retryAfter && !S.autoRetried && !e.retryUntil ? (e.retryUntil = Date.now() + e.retryAfter * 1000) : e.retryUntil;
    if (autoRetry && autoRetry > Date.now()) actions += '<span class="err-countdown" id="err-countdown" aria-live="off"></span>';
    if (e.signin) actions += '<a class="btn btn-primary btn-sm" href="/auth">' + esc(t("nav.signin")) + "</a>";
    box.innerHTML = '<div class="err-head"><span class="err-icon">' + icons.alert + '</span><div class="grow"><h3 id="err-title" tabindex="-1">' + esc(e.title) + "</h3>" + (e.body ? "<p>" + esc(e.body) + "</p>" : "") + where + "</div></div>" +
      (actions ? '<div class="err-actions">' + actions + "</div>" : "") +
      (e.detail ? '<details class="err-detail"><summary>' + icons.chevron + "<span>" + esc(t("err.details")) + "</span></summary><pre lang=\"en\">" + esc(e.detail) + "</pre></details>" : "");
    show("state-error");
    if (!e.compile) announce(e.title + ". " + (e.body || ""));
    else announce("");
    if (e.compile) {
      setMode("geo");
      editor.setError({ line: e.diagnosis && e.diagnosis.line, col: e.diagnosis && e.diagnosis.col, len: e.diagnosis && e.diagnosis.len, message: e.body });
    }
    var ae = document.activeElement;
    if (focusHead || !ae || ae === document.body) $("err-title").focus({ preventScroll: true });
    var g = $("err-goto");
    if (g) g.addEventListener("click", function () { editor.focusError(); });
    var rt = $("err-retry");
    if (rt) rt.addEventListener("click", solve);
    var cd = $("err-countdown");
    if (cd) {
      var tickRetry = function () {
        var left = Math.ceil((e.retryUntil - Date.now()) / 1000);
        if (left <= 0) {
          clearInterval(S.retryTimer);
          S.retryTimer = null;
          if (S.lastError === e && !$("state-error").hidden && !S.busy) { S.autoRetrying = true; solve(); }
          return;
        }
        cd.textContent = t("err.retry_in", { s: left });
      };
      tickRetry();
      S.retryTimer = setInterval(tickRetry, 250);
    }
    scrollToStatus();
  }

  var announceTimer = null;
  function announce(msg) {
    var a = $("announce");
    clearTimeout(announceTimer);
    a.textContent = "";
    if (msg) announceTimer = setTimeout(function () { a.textContent = msg; }, 50);
  }
  var statusTimer = null;
  function announceStatus(msg) {
    var a = $("announce-status");
    clearTimeout(statusTimer);
    a.textContent = "";
    if (msg) statusTimer = setTimeout(function () { a.textContent = msg; }, 400);
  }
  function scrollToStatus() {
    if (window.matchMedia("(max-width: 1199px)").matches) {
      var el = $("status-area");
      var top = el.getBoundingClientRect().top + window.scrollY - 72;
      if (Math.abs(window.scrollY - top) > 40) window.scrollTo({ top: top, behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
    }
  }

  // ---------------------------------------------------------------- verdict --
  function verdictModel(sol) {
    var v = sol.view || {}, note = v.note || {};
    var timeLimited = note.key === "time_limit";
    switch (sol.status) {
      case "proved":
        if (!hasSteps(sol)) return { tone: "proved", icon: "check", head: t("v.proved"), text: t("v.proved.immediate") };
        if (v.as_drawn) return { tone: "proved", icon: "check", head: t("v.proved.drawn"), text: t("v.proved.drawn.x") };
        return { tone: "proved", icon: "check", head: t("v.proved"), text: t("v.proved.x") };
      case "refuted": return { tone: "false", icon: "cross", head: t("v.false"), text: t("v.false.x") };
      case "holds-numerically": return { tone: "unproved", icon: "approx", head: t("v.numeric"), text: tp("v.numeric.x", sol.numeric_samples || (v.evidence && v.evidence.samples) || 0) };
      default:
        if (timeLimited) return { tone: "neutral", icon: "clock", head: t("v.time"), text: t("v.time.x", { s: Math.round(note.secs || S.deadline) }) };
        var text = t("v.not.x");
        if (note.key === "budget" && note.runs != null) text = t("v.not.budget", { runs: tp("runs", note.runs) });
        else if (["metric_error", "unsound", "replay"].indexOf(note.key) >= 0) text = t("v.not." + note.key);
        return { tone: "neutral", icon: "minus", head: t("v.not"), text: text };
    }
  }
  function hasSteps(sol) {
    return !!(sol.view && sol.view.proof && sol.view.proof.steps && sol.view.proof.steps.length);
  }
  function stepCount(sol) {
    if (sol.status !== "proved" || !sol.view || !sol.view.proof) return 0;
    return (sol.view.proof.steps || []).filter(function (s) { return s.kind === "step"; }).length;
  }
  function methodText(sol) {
    if (sol.status === "holds-numerically") return t("meta.method.numeric");
    if (sol.status === "refuted") return t(sol.view && sol.view.counterexample ? "meta.method.counter" : "meta.method.numeric");
    if (sol.method === "euclidean") return sol.status === "proved" ? t("meta.method.euclid") : "";
    var n = (sol.aux_constructions || []).length;
    if (n) return tp("meta.method.aux", n);
    return sol.method === "aux-search" ? t("meta.method.aux_search") : t("meta.method.ddar");
  }
  function counterText(c) {
    if (!c) return "";
    var n = window.i18n.fmtNum;
    var digits = c.kind === "values" || c.kind === "length" || c.kind === "ratios" ? 4 : 1;
    var L = c.labels || [];
    var exact = (c.kind === "angle" || c.kind === "values") && typeof c.rhs === "number" && c.rhs % 1 === 0;
    return t("counter." + c.kind, { a: L[0] || "", b: L[1] || "", lhs: n(c.lhs, digits), rhs: n(c.rhs, exact ? 0 : digits) });
  }

  function renderVerdict(sol) {
    var m = verdictModel(sol), v = sol.view || {};
    var nSteps = stepCount(sol);
    var meta = [
      methodText(sol),
      sol.search === "shortest" ? t("meta.shortest_search", { t: GS.fmtSecs(sol.elapsed_secs) }) : GS.fmtSecs(sol.elapsed_secs),
      nSteps ? tp("meta.steps", nSteps) : "",
      sol.status === "holds-numerically" && sol.numeric_samples ? tp("meta.samples", sol.numeric_samples) : "",
      sol.status === "proved" && v.as_drawn ? t("meta.as_drawn") : "",
    ].filter(Boolean).map(function (x) { return "<span>" + esc(x) + "</span>"; });
    var guest = S.status && S.status.guest && !S.status.signed_in;
    var guestNote = guest ? '<p class="hint guest-note"><span class="hint-ic" aria-hidden="true">' + icons.info + "</span><span>" + esc(t("hist.guest")) + ' <a href="/auth?mode=register">' + esc(t("nav.create")) + "</a></span></p>" : "";
    var counter = v.counterexample ? '<p class="counter"><strong>' + esc(t("counter.title")) + ".</strong> " + GS.math(counterText(v.counterexample), true) + "</p>" : "";
    var actions = '<div class="verdict-actions">' +
      (sol.status === "proved" && hasSteps(sol) ? '<button type="button" class="btn btn-secondary btn-sm" id="copy-proof">' + icons.copy + "<span>" + esc(t("action.copy_proof")) + "</span></button>" : "") +
      '<button type="button" class="btn btn-secondary btn-sm" id="copy-geo">' + icons.code + "<span>" + esc(t("action.copy_geo")) + "</span></button>" +
      '<div class="menu-wrap"><button type="button" class="btn btn-secondary btn-sm" id="export-btn" aria-haspopup="menu" aria-expanded="false" aria-controls="export-menu">' + icons.download + "<span>" + esc(t("action.export")) + "</span>" + icons.chevron + "</button>" +
      '<div class="menu" id="export-menu" role="menu" aria-labelledby="export-btn" hidden>' +
      '<button type="button" role="menuitem" data-export="pdf">' + icons.file + "<span>" + esc(t("export.pdf")) + "</span></button>" +
      '<button type="button" role="menuitem" data-export="png">' + icons.image + "<span>" + esc(t("export.png")) + "</span></button>" +
      '<button type="button" role="menuitem" data-export="svg">' + icons.download + "<span>" + esc(t("export.svg")) + "</span></button>" +
      "</div></div></div>";
    var box = $("verdict");
    box.className = "verdict card tone-" + m.tone;
    box.innerHTML = '<div class="verdict-main"><span class="verdict-icon">' + icons[m.icon] + "</span><div class=\"grow\">" +
      '<h3 class="verdict-head" id="verdict-head" tabindex="-1">' + esc(m.head) + "</h3>" +
      '<p class="verdict-text">' + esc(m.text) + "</p>" + counter +
      '<p class="verdict-meta">' + meta.join("") + "</p>" +
      '<p class="refine" id="refine" hidden></p>' + guestNote +
      "</div></div>" + actions;
    show("verdict");
    wireVerdictActions();
    return m;
  }

  function wireVerdictActions() {
    var cp = $("copy-proof");
    if (cp) cp.addEventListener("click", function () { copyText(proofText()); });
    $("copy-geo").addEventListener("click", function () { copyText(S.sol ? S.sol.input : editor.get()); });
    var btn = $("export-btn"), menu = $("export-menu");
    function close(focus) { menu.hidden = true; btn.setAttribute("aria-expanded", "false"); if (focus) btn.focus(); }
    paintExportItems();
    btn.addEventListener("click", function () {
      var open = menu.hidden;
      if (open) { paintExportItems(); placeMenu(menu, false); } else menu.hidden = true;
      btn.setAttribute("aria-expanded", open ? "true" : "false");
      if (open) menu.querySelector("[role=menuitem]").focus();
    });
    menu.addEventListener("keydown", function (e) {
      var items = Array.prototype.slice.call(menu.querySelectorAll("[role=menuitem]"));
      var i = items.indexOf(document.activeElement);
      if (e.key === "ArrowDown") { e.preventDefault(); items[(i + 1) % items.length].focus(); }
      else if (e.key === "ArrowUp") { e.preventDefault(); items[(i - 1 + items.length) % items.length].focus(); }
      else if (e.key === "Home") { e.preventDefault(); items[0].focus(); }
      else if (e.key === "End") { e.preventDefault(); items[items.length - 1].focus(); }
      else if (e.key === "Escape") { e.preventDefault(); close(true); }
      else if (e.key === "Tab") close();
    });
    menu.addEventListener("click", function (e) {
      var b = e.target.closest("[data-export]");
      if (!b) return;
      close(true);
      if (b.getAttribute("aria-disabled") === "true") { GS.toast(t("export.busy", { fmt: String(S.exporting).toUpperCase() }), { ms: 5000 }); return; }
      exportAs(b.getAttribute("data-export"));
    });
    if (!wireVerdictActions.outside) {
      wireVerdictActions.outside = true;
      document.addEventListener("click", function (e) {
        var m = $("export-menu");
        if (m && !m.hidden && !e.target.closest(".verdict .menu-wrap")) { m.hidden = true; $("export-btn").setAttribute("aria-expanded", "false"); }
      });
    }
  }

  // -------------------------------------------------------------- solution --
  function renderSolution(sol, opts) {
    opts = opts || {};
    S.sol = sol;
    S.geo = sol.input;
    S.baseline = sol.input;
    var v = sol.view || {};
    var m = renderVerdict(sol);
    renderStatement(sol);
    renderProof(sol);
    renderDetails(sol);
    document.body.classList.add("has-result");
    renderFigure(sol);
    $("proof-area").hidden = false;
    if (opts.announce) announce(m.head + ". " + m.text);
    if (opts.focus) {
      var narrow = window.matchMedia("(max-width: 1199px)").matches;
      if (narrow) scrollToStatus();
      var ae = document.activeElement;
      var h = $("verdict-head");
      if (h && (narrow || !ae || ae === document.body || ae === $("solve") || $("state-solving").contains(ae))) h.focus({ preventScroll: true });
    }
    void v;
  }

  function renderStatement(sol) {
    var v = sol.view || {};
    var given = (v.given || []).map(function (f) { return '<li class="math" tabindex="-1" data-points="' + esc((f.points || []).join(" ")) + '">' + GS.fact(f) + "</li>"; }).join("");
    var aux = (v.aux || []).map(function (a) {
      return '<li class="math" tabindex="-1" data-points="' + esc(a.name) + '"><span class="aux-name">' + GS.math(a.name) + "</span>: " + GS.math(auxText(a)) + "</li>";
    }).join("");
    var helpers = (v.helpers || []).map(function (a) {
      return '<li class="math" tabindex="-1" data-points="' + esc(a.name) + '">' + GS.math(a.name) + ": " + GS.math(auxText(a)) + "</li>";
    }).join("");
    var html = '<h2 class="sr-only" id="st-h">' + esc(t("st.statement")) + "</h2>";
    if (sol.title) html += '<h3 class="st-title" title="' + esc(sol.title) + '">' + esc(sol.title) + "</h3>";
    if (given) html += '<div class="st-block"><h3 class="label">' + esc(t("st.given")) + '</h3><ul class="facts" role="list">' + given + "</ul></div>";
    if (v.goal) html += '<div class="st-block st-goal"><h3 class="label">' + esc(t("st.prove")) + '</h3><p class="math goal" tabindex="-1" data-points="' + esc((v.goal.points || []).join(" ")) + '">' + GS.fact(v.goal) + "</p></div>";
    if (helpers) html += '<div class="st-block st-helpers"><h3 class="label">' + esc(t("st.helpers")) + '</h3><ul class="facts" role="list">' + helpers + '</ul><p class="hint">' + esc(t("st.helpers.hint")) + "</p></div>";
    if (aux) html += '<div class="st-block st-aux"><h3 class="label">' + esc(t("st.aux")) + '</h3><ul class="facts" role="list">' + aux + '</ul><p class="hint">' + esc(t("st.aux.hint")) + "</p></div>";
    var nItems = (v.given || []).length + (v.helpers || []).length + (v.aux || []).length + (v.goal ? 1 : 0);
    if (nItems > 1) html += '<p class="hint kbd-hint" id="st-kbd">' + esc(t("st.kbd")) + "</p>";
    var box = $("statement");
    box.innerHTML = html;
    box.hidden = !(sol.title || given || v.goal || aux || helpers);
    var items = Array.prototype.slice.call(box.querySelectorAll("[data-points]"));
    var facts = (v.given || []).concat(v.goal ? [v.goal] : []);
    items.forEach(function (el, i) {
      var pts = el.getAttribute("data-points").split(" ");
      var fs = facts[i] ? [facts[i]] : null;
      el.tabIndex = i === 0 ? 0 : -1;
      if (items.length > 1) el.setAttribute("aria-describedby", "st-kbd");
      el.addEventListener("mouseenter", function () { viewer.highlight(pts, fs); });
      el.addEventListener("mouseleave", function () { if (document.activeElement !== el) viewer.highlight(null); });
      el.addEventListener("focus", function () {
        items.forEach(function (x) { x.tabIndex = x === el ? 0 : -1; });
        viewer.highlight(pts, fs);
        if (stepsApi) stepsApi.markPoint(pts.length === 1 ? pts[0] : null);
      });
      el.addEventListener("blur", function () { viewer.highlight(null); if (stepsApi) stepsApi.markPoint(null); });
      el.addEventListener("keydown", function (e) {
        var j = null;
        if (e.key === "ArrowDown") j = Math.min(items.length - 1, i + 1);
        else if (e.key === "ArrowUp") j = Math.max(0, i - 1);
        else if (e.key === "Home") j = 0;
        else if (e.key === "End") j = items.length - 1;
        if (j != null) { e.preventDefault(); items[j].focus(); }
      });
    });
  }

  function auxText(a) {
    var key = "aux." + a.kind;
    var args = (a.args || []).map(function (x) {
      var mm = /^(circumcircle|circle|para|perp|tangent_at|isogonal)\((.*)\)$/.exec(String(x).trim());
      if (mm) {
        var parts = mm[2].split(mm[1] === "isogonal" ? " in " : ",").map(function (s) { return s.trim().replace(/^cent(re|er) /, ""); });
        var k2 = /circ/.test(mm[1]) ? "aux." + mm[1] : "aux.line." + mm[1];
        var s2 = t(k2);
        parts.forEach(function (p, i) { s2 = s2.split("{" + i + "}").join(p); });
        if (!/\{\d\}/.test(s2)) return s2;
      }
      return x;
    });
    if (!window.i18n.has(key)) return a.text;
    if (a.kind === "intersect" && (a.args || []).length === 2 && /^circ/.test(a.args[1]) && a.args[0].indexOf(a.name) < 0) {
      var inner = (/\((.*)\)$/.exec(a.args[1]) || [])[1] || "";
      var on = inner.split(",").map(function (x) { return x.trim(); });
      if (on.some(function (p) { return a.args[0].indexOf(p) >= 0; })) key = "aux.intersect2";
    }
    if (key === "aux.intersect2" && /^[a-z_]+\(/.test(String((a.args || [])[0] || "").trim())) key = "aux.intersect2_shape";
    var s = t(key);
    if (a.kind === "midpoint" || a.kind === "circumcenter" || a.kind === "orthocenter" || a.kind === "parallelogram") {
      var flat = args.join(",").split(",").map(function (x) { return x.trim(); });
      flat.forEach(function (p, i) { s = s.split("{" + i + "}").join(p); });
    } else {
      args.forEach(function (p, i) { s = s.split("{" + i + "}").join(p); });
    }
    if (/\{\d\}/.test(s)) return a.text;
    return a.same ? s + t("aux.same", { p: a.same }) : s;
  }

  var stepsApi = null;
  function renderProof(sol) {
    var v = sol.view || {};
    var proved = sol.status === "proved" && v.proof && v.proof.steps && v.proof.steps.length;
    $("steps").hidden = !proved;
    $("steps-none").hidden = !!proved;
    $("steps-none").textContent = t("proof.none");
    var proofCard = $("steps").closest(".proof");
    proofCard.hidden = !proved;
    if (proved) stepsApi = GS.renderSteps($("steps"), v.proof, { focus: function (pts, facts) { viewer.highlight(pts, facts); }, describedBy: "proof-kbd" });
    else { $("steps").innerHTML = ""; stepsApi = null; }
    var aiOK = !!(proved && S.status && S.status.translate_logged_in && S.status.translate_installed);
    $("proof-tabs").hidden = !aiOK;
    var panel = $("proof-steps-panel");
    if (aiOK) { panel.setAttribute("role", "tabpanel"); panel.setAttribute("aria-labelledby", "ptab-steps"); }
    else { panel.removeAttribute("role"); panel.removeAttribute("aria-labelledby"); }
    selectProofTab("steps");
  }

  function selectProofTab(which) {
    var steps = which === "steps";
    $("ptab-steps").setAttribute("aria-selected", steps ? "true" : "false");
    $("ptab-ai").setAttribute("aria-selected", steps ? "false" : "true");
    $("ptab-steps").tabIndex = steps ? 0 : -1;
    $("ptab-ai").tabIndex = steps ? -1 : 0;
    $("proof-steps-panel").hidden = !steps;
    $("proof-ai-panel").hidden = steps;
    if (!steps) loadAi(false);
  }

  function proofText() {
    var sol = S.sol;
    if (!sol || !sol.view) return "";
    var v = sol.view, out = [];
    if (sol.title) out.push(sol.title, "");
    if (v.given && v.given.length) { out.push(t("st.given") + ":"); v.given.forEach(function (f) { out.push("- " + GS.factText(f)); }); out.push(""); }
    if (v.goal) out.push(t("st.prove") + ": " + GS.factText(v.goal), "");
    if (v.aux && v.aux.length) { out.push(t("st.aux") + ":"); v.aux.forEach(function (a) { out.push("- " + a.name + ": " + auxText(a)); }); out.push(""); }
    out.push(t("proof.title") + ":");
    (v.proof.steps || []).forEach(function (s) {
      out.push(s.n + ". " + GS.factText(s.fact) + " — " + GS.ruleLabel(s) + (s.deps && s.deps.length ? " [" + s.deps.join(", ") + "]" : ""));
      (s.subs || []).forEach(function (u) { out.push("   • " + GS.factText(u.fact) + " — " + GS.ruleLabel(u)); });
    });
    if (v.proof.conclusion) out.push("∎ " + GS.factText(v.proof.conclusion));
    return out.join("\n");
  }

  function englishProof() {
    var v = S.sol.view, lines = [];
    (v.proof.steps || []).forEach(function (s) {
      lines.push(s.n + ". " + GS.factText(s.fact) + " (" + GS.ruleLabel(s) + (s.deps && s.deps.length ? "; from " + s.deps.join(", ") : "") + ")");
      (s.subs || []).forEach(function (u) { lines.push("   - " + GS.factText(u.fact) + " (" + GS.ruleLabel(u) + ")"); });
    });
    if (v.proof.conclusion) lines.push("QED: " + GS.factText(v.proof.conclusion));
    return lines.join("\n");
  }

  function loadAi(force) {
    var sol = S.sol;
    if (!sol) return;
    var lang = window.i18n.current();
    var key = (sol.id || sol.input) + "|" + lang;
    var box = $("ai-text");
    if (!force && S.aiCache[key]) { if (box.innerHTML !== S.aiCache[key]) box.innerHTML = S.aiCache[key]; return; }
    var seq = ++S.aiSeq;
    box.innerHTML = '<p class="ai-loading"><span class="spinner" aria-hidden="true"></span>' + esc(t("proof.ai.loading")) + "</p>";
    var v = sol.view;
    var problem = ["Given: " + (v.given || []).map(GS.factText).join("; "), v.goal ? "Prove: " + GS.factText(v.goal) : ""].join("\n");
    var aux = (v.aux || []).map(function (a) { return a.name + " = " + a.text; });
    api("/api/humanize", { method: "POST", body: { problem: problem, proof: englishProof(), aux: aux, lang: lang } }).then(function (r) {
      if (seq !== S.aiSeq) return;
      if (!r.ok || !r.data.proof) throw new Error("ai");
      var html = markdown(r.data.proof);
      S.aiCache[key] = html;
      box.innerHTML = html;
      announceStatus(t("proof.ai.ready"));
    }).catch(function () {
      if (seq !== S.aiSeq) return;
      box.innerHTML = '<p class="muted">' + esc(t("proof.ai.fail")) + "</p>";
      announceStatus(t("proof.ai.fail"));
    });
  }
  function delatex(s) {
    var map = { angle: "\u2220", perp: "\u27c2", parallel: "\u2225", triangle: "\u25b3", cdot: "\u00b7", circ: "\u00b0", sim: "\u223c", cong: "\u2245", ne: "\u2260", neq: "\u2260", le: "\u2264", ge: "\u2265", Rightarrow: "\u21d2", implies: "\u21d2", times: "\u00d7", Omega: "\u03a9", omega: "\u03c9", quad: " " };
    return s.replace(/\$\$?([^$]+?)\$\$?/g, "$1")
      .replace(/\^\{?\\circ\}?/g, "\u00b0")
      .replace(/\\([A-Za-z]+)\s?/g, function (m, w) { return map[w] != null ? map[w] : w; })
      .replace(/[{}]/g, "");
  }
  function markdown(md) {
    return delatex(md || "").split(/\n{2,}/).map(function (par) {
      var p = esc(par.trim());
      p = p.replace(/\*\*([\s\S]+?)\*\*/g, "<strong>$1</strong>").replace(/\*([^*]+?)\*/g, "<em>$1</em>").replace(/\n/g, "<br>");
      return p ? "<p>" + p + "</p>" : "";
    }).join("");
  }

  function noteText(note) {
    var k = "note.sentence." + note.key;
    if (!window.i18n.has(k) && !window.i18n.has(k + ".other")) return "";
    var vars = { secs: note.secs != null ? Math.round(note.secs) : S.deadline, runs: note.runs != null ? tp("runs", note.runs) : "?" };
    if (note.n != null) return tp(k, note.n, vars);
    return t(k + ".other", vars) === k + ".other" ? t(k, vars) : t(k + ".other", vars);
  }
  function renderDetails(sol) {
    var v = sol.view || {};
    var note = "";
    if (v.note && v.note.raw) {
      var said = noteText(v.note);
      note = '<div class="dt"><h3 class="label">' + esc(t("details.note")) + "</h3>" + (said ? "<p>" + esc(said) + "</p>" : "") +
        '<details class="raw-note"><summary>' + icons.chevron + "<span>" + esc(t("details.raw")) + '</span></summary><p class="mono small" lang="en">' + esc(v.note.raw) + "</p></details></div>";
    }
    $("details-body").innerHTML = '<div class="dt"><h3 class="label">' + esc(t("details.geo")) + '</h3><pre class="code">' + esc(sol.input) + "</pre></div>" + note;
  }

  function figureAria(sol) {
    var v = sol.view || {};
    var pts = (v.points || []).map(function (p) { return p.name; }).join(", ");
    return t("fig.aria", { pts: pts, goal: v.goal ? GS.factText(v.goal) : "—" });
  }
  function renderFigure(sol) {
    if (!sol.svg) { fitViewportToFigure(""); viewer.setSvg(""); $("fig-empty").hidden = false; figTools(false); return; }
    $("fig-empty").hidden = true;
    figTools(true);
    fitViewportToFigure(sol.svg);
    viewer.setSvg(sol.svg, figureAria(sol));
    $("fig-legend").hidden = false;
    $("fig-legend").querySelector(".lg-aux").hidden = !((sol.view && sol.view.aux) || []).length;
    $("fig-legend").querySelector(".lg-goal").hidden = !viewer.svg || !viewer.svg.querySelector(".f-goal");
  }
  var fittedSvg = "";
  function fitViewportToFigure(svg) {
    fittedSvg = svg || "";
    var vp = $("fig-viewport"), frame = $("fig-frame");
    vp.style.height = "";
    frame.style.removeProperty("--fig-fit");
    var m = /viewBox="([^"]+)"/.exec(fittedSvg);
    var b = m ? m[1].split(/\s+/).map(Number) : null;
    var w = vp.getBoundingClientRect().width;
    if (!b || !(b[2] > 0) || !(b[3] > 0) || !w) return;
    if (!window.matchMedia("(max-width: 1023px)").matches) {
      var spare = vp.getBoundingClientRect().height - Math.max(320, w * b[3] / b[2] + 24);
      if (spare > 8) frame.style.setProperty("--fig-fit", Math.round(frame.getBoundingClientRect().height - spare) + "px");
      return;
    }
    var ratio = Math.max(0.6, Math.min(1.25, b[3] / b[2]));
    vp.style.height = Math.round(w * ratio) + "px";
  }

  var refitTimer = 0;
  window.addEventListener("resize", function () {
    clearTimeout(refitTimer);
    refitTimer = setTimeout(function () { if (fittedSvg) fitViewportToFigure(fittedSvg); }, 120);
  });

  // ------------------------------------------------------- shorter proofs --
  function refineShorter(first) {
    var ctl = new AbortController();
    S.refining = ctl;
    var el = $("refine");
    el.hidden = false;
    el.innerHTML = '<span class="spinner" aria-hidden="true"></span><span>' + esc(t("shorter.searching")) + '</span> <button type="button" class="link-btn" id="refine-stop">' + esc(t("shorter.stop")) + "</button>";
    $("refine-stop").addEventListener("click", stopRefining);
    announceStatus(t("shorter.searching"));
    api("/api/solve", { method: "POST", body: { input: first.input, title: first.title, best: true, budget_secs: 20, record: false }, signal: ctl.signal }).then(function (r) {
      if (S.refining !== ctl) return;
      S.refining = null;
      if (S.sol !== first) return;
      var better = r.ok && r.data.status === "proved" && stepCount(r.data) && stepCount(r.data) < stepCount(first);
      if (!r.ok) {
        if (r.status === 401) authLost();
        var e1 = $("refine");
        var key = r.status === 503 || r.status === 429 ? "shorter.busy" : "shorter.failed";
        if (e1) e1.innerHTML = "<span>" + esc(t(key)) + "</span>";
        if (e1 && e1.contains(document.activeElement)) focusVerdictHead();
        announceStatus(t(key));
        return;
      }
      if (better) {
        var msg = t("shorter.found", { n: tp("meta.steps", stepCount(r.data)), m: tp("meta.steps", stepCount(first)) });
        var keep = focusMark();
        renderSolution(r.data, {});
        restoreFocus(keep);
        GS.toast(msg);
        if (first.history_id && r.data.id) {
          r.data.history_id = first.history_id;
          api("/api/history/" + first.history_id, { method: "PUT", body: { id: r.data.id } }).then(function () { loadHistory(); });
        }
      } else {
        var e2 = $("refine");
        var hadFocus = e2 && e2.contains(document.activeElement);
        if (e2) { e2.innerHTML = '<span>' + esc(t("shorter.none")) + "</span>"; }
        if (hadFocus) focusVerdictHead();
        announceStatus(t("shorter.none"));
      }
    }).catch(function (err) {
      var mine = S.refining === ctl;
      if (mine) S.refining = null;
      var e3 = $("refine");
      if (e3 && e3.contains(document.activeElement)) focusVerdictHead();
      if (!e3) return;
      if (mine && S.sol === first && !(err && err.name === "AbortError")) {
        e3.innerHTML = "<span>" + esc(t("shorter.failed")) + "</span>";
        announceStatus(t("shorter.failed"));
      } else e3.hidden = true;
    });
  }
  function stopRefining() {
    if (!S.refining) return;
    S.refining.abort();
    S.refining = null;
    var el = $("refine");
    if (el && el.contains(document.activeElement)) focusVerdictHead();
    if (el) el.hidden = true;
  }
  function focusVerdictHead() {
    var h = $("verdict-head");
    if (h) h.focus({ preventScroll: true });
  }
  /** Where keyboard focus is inside the result (a control id, or a step
   * index), so it can be put back after the result is re-rendered. */
  function focusMark() {
    var ae = document.activeElement;
    if (!ae || ae === document.body) return null;
    if (ae.id && ($("verdict").contains(ae) || $("proof-area").contains(ae))) return { id: ae.id };
    var li = ae.closest && ae.closest("#steps .step");
    if (li) return { step: Array.prototype.indexOf.call($("steps").querySelectorAll(".step:not([hidden])"), li) };
    if ($("verdict").contains(ae) || $("proof-area").contains(ae)) return { head: true };
    return null;
  }
  function restoreFocus(mark) {
    if (!mark) return;
    var el = mark.id ? $(mark.id) : null;
    if (!el && mark.step != null) {
      var steps = $("steps").querySelectorAll(".step:not([hidden])");
      el = steps.length ? steps[Math.min(mark.step, steps.length - 1)] : null;
    }
    if (el && !el.hidden && el.offsetParent !== null) el.focus({ preventScroll: true });
    else focusVerdictHead();
  }

  // ---------------------------------------------------------------- export --
  function download(blob, name) {
    var url = URL.createObjectURL(blob);
    var a = document.createElement("a");
    a.href = url; a.download = name;
    document.body.appendChild(a); a.click(); a.remove();
    setTimeout(function () { URL.revokeObjectURL(url); }, 120000);
  }
  function standalone() {
    try { return navigator.standalone === true || matchMedia("(display-mode: standalone)").matches; } catch (e) { return false; }
  }
  function shareableFile(blob, name) {
    if (!navigator.share || !navigator.canShare) return null;
    if (!coarse() && !standalone()) return null;
    try {
      var file = new File([blob], name, { type: blob.type || "application/octet-stream" });
      return navigator.canShare({ files: [file] }) ? file : null;
    } catch (e) { return null; }
  }
  function android() {
    try { return /Android/i.test(navigator.userAgent) || (navigator.userAgentData && navigator.userAgentData.platform === "Android"); } catch (e) { return false; }
  }
  function shareFile(file, blob, label, saved) {
    return navigator.share({ files: [file] }).catch(function (e) {
      if (e && e.name === "AbortError") return;
      if (saved) { GS.toast(t("export.share_fail")); return; }
      download(blob, file.name);
      GS.toast(t("export.done", { fmt: label }));
    });
  }
  function deliver(blob, name, label, inGesture) {
    var file = shareableFile(blob, name);
    if (!file || android()) {
      download(blob, name);
      if (file) GS.toast(t("export.done", { fmt: label }), { ms: 10000, action: t("share"), onAction: function () { shareFile(file, blob, label, true); } });
      else GS.toast(t("export.done", { fmt: label }));
      return;
    }
    if (inGesture) { shareFile(file, blob, label); return; }
    GS.toast(t("export.ready", { fmt: label }), { ms: 120000, action: t("export.share"), onAction: function () { shareFile(file, blob, label); } });
  }
  function slug(s) {
    return String(s || "").normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase()
      .replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 48).replace(/-+$/, "");
  }
  function exportName(sol, fmt) {
    var known = { proved: 1, refuted: 1, "holds-numerically": 1 }[sol.status] ? sol.status : "not-proved";
    return fileName(sol, slug(t("export.suffix." + known))) + "." + fmt;
  }
  function fileName(sol, suffix) {
    var title = slug(sol.title);
    return "geosolver-" + (title ? title + "-" : "") + suffix;
  }
  function exportAs(fmt) {
    var sol = S.sol;
    if (!sol) return;
    var label = fmt.toUpperCase();
    if (fmt === "svg") {
      deliver(new Blob([sol.svg], { type: "image/svg+xml" }), fileName(sol, slug(t("export.suffix.figure"))) + ".svg", "SVG", true);
      return;
    }
    if (S.exporting) return;
    var ctl = new AbortController();
    S.exporting = fmt;
    S.exportCtl = ctl;
    paintExportItems();
    var tst = GS.toast(t("export.preparing", { fmt: label }), { ms: 60000 });
    var ticker = null;
    var closeToast = function () { clearInterval(ticker); if (tst) tst.close(); tst = null; };
    var post = function (body) {
      return fetch("/api/export", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body), signal: ctl.signal });
    };
    var signed = function () {
      if (!sol.signed) return Promise.resolve(null);
      return post({ signed: sol.signed.body, sig: sol.signed.sig, format: fmt }).then(function (res) { return res.status === 410 ? null : res; });
    };
    var recache = function () {
      if (!sol.history_id || !(S.status && S.status.signed_in)) return Promise.resolve(null);
      return api("/api/history/" + sol.history_id, { signal: ctl.signal }).then(function (r) {
        return r.ok && r.data && r.data.id ? r.data.id : null;
      }, function (e) { if (e && e.name === "AbortError") throw e; return null; });
    };
    var resolve = function () {
      closeToast();
      var started = Date.now(), max = S.deadline + 5;
      var text = function () { return t("export.resolving", { fmt: label, t: Math.floor((Date.now() - started) / 1000) + "\u00a0s", max: max + "\u00a0s" }); };
      tst = GS.toast(text(), { ms: (max + 60) * 1000, action: t("cancel"), onAction: function () { ctl.abort(); } });
      ticker = setInterval(function () { if (tst && tst.el) tst.el.firstChild.textContent = text(); }, 1000);
      return post({ input: sol.input, title: sol.title || null, format: fmt });
    };
    var fail = function (msg) {
      closeToast();
      GS.toast(t("export.failed", { msg: msg }), { ms: 7000 });
    };
    post({ id: sol.id, format: fmt }).then(function (res) {
      if (res.status !== 410) return res;
      return signed().then(function (r2) {
        if (r2) return r2;
        return recache().then(function (id) {
          if (!id) return resolve();
          if (S.sol === sol) sol.id = id;
          return post({ id: id, format: fmt });
        });
      });
    }).then(function (res) {
      if (res.ok) return res.blob().then(function (b) {
        closeToast();
        deliver(b.type ? b : new Blob([b], { type: fmt === "pdf" ? "application/pdf" : "image/png" }), exportName(sol, fmt), label, false);
      });
      var ct = res.headers.get("content-type") || "";
      return res.text().catch(function () { return ""; }).then(function (x) {
        var d = {};
        if (ct.indexOf("json") >= 0) { try { d = JSON.parse(x) || {}; } catch (e) { d = {}; } }
        if (GS.isGate(res.status, d)) { closeToast(); return GS.toGate(); }
        if (res.status === 401 && S.status && S.status.signed_in) {
          closeToast();
          sessionEnded();
          GS.toast(t("export.auth"), { ms: 9000 });
          return;
        }
        var e = httpError({ status: res.status, data: d }, "export");
        fail(errText(e, "body") || errText(e, "title"));
      });
    }).catch(function (e) {
      if (e && e.name === "AbortError") { closeToast(); GS.toast(t("export.cancelled")); return; }
      fail(t("err.body.network"));
    }).then(function () {
      clearInterval(ticker);
      if (S.exportCtl === ctl) { S.exporting = null; S.exportCtl = null; }
      paintExportItems();
    });
  }
  function paintExportItems() {
    var menu = $("export-menu");
    if (!menu) return;
    menu.querySelectorAll('[data-export="pdf"], [data-export="png"]').forEach(function (b) {
      if (S.exporting) b.setAttribute("aria-disabled", "true"); else b.removeAttribute("aria-disabled");
    });
  }
  function copyText(text) {
    var done = function () { GS.toast(t("copied")); };
    var fail = function () {
      if (navigator.share) GS.toast(t("copy_fail"), { ms: 10000, action: t("share"), onAction: function () { navigator.share({ text: text }).catch(function () {}); } });
      else GS.toast(t("copy_fail"));
    };
    var legacy = function () {
      var prev = document.activeElement;
      var ta = document.createElement("textarea");
      ta.value = text; ta.setAttribute("readonly", "");
      ta.style.position = "fixed"; ta.style.top = "0"; ta.style.left = "0"; ta.style.opacity = "0"; ta.style.fontSize = "16px";
      document.body.appendChild(ta);
      ta.focus({ preventScroll: true }); ta.select(); ta.setSelectionRange(0, text.length);
      var ok = false;
      try { ok = document.execCommand("copy"); } catch (e) { ok = false; }
      ta.remove();
      if (prev && prev !== document.body && prev.focus) prev.focus({ preventScroll: true });
      return ok;
    };
    if (navigator.clipboard && navigator.clipboard.writeText && window.isSecureContext) {
      navigator.clipboard.writeText(text).then(done, function () { legacy() ? done() : fail(); });
      return;
    }
    legacy() ? done() : fail();
  }

  // --------------------------------------------------------------- history --
  function histTitle(r) {
    if (r.title) return r.title;
    if (r.goal) return t("st.prove") + ": " + GS.factText(r.goal);
    var first = (r.input || "").split("\n").filter(function (l) { return l.trim() && l.trim()[0] !== "#"; })[0];
    return first || t("hist.untitled");
  }
  function histTitleHtml(r) {
    if (!r.title && r.goal) return esc(t("st.prove")) + ": " + GS.fact(r.goal);
    return esc(histTitle(r));
  }
  function histStatus(r) {
    if (r.status === "not-proved" && r.note === "time_limit") return "time-limit";
    if (r.status === "proved" && r.as_drawn) return "proved-drawn";
    if (r.status) return r.status;
    if (r.proved && r.method === "euclidean") return "legacy";
    return r.proved ? "proved" : "not-proved";
  }
  var TONE = { proved: "proved", "proved-drawn": "proved", refuted: "false", "holds-numerically": "unproved", "not-proved": "neutral", "time-limit": "neutral", legacy: "neutral" };
  var ICON = { proved: "check", "proved-drawn": "check", refuted: "cross", "holds-numerically": "approx", "not-proved": "minus", "time-limit": "clock", legacy: "minus" };
  var FILTER_OF = { "time-limit": "not-proved", "proved-drawn": "proved" };
  function relTime(sec) {
    var diff = sec - (Date.now() + (S.skew || 0)) / 1000;
    var rtf = new Intl.RelativeTimeFormat(window.i18n.locale(), { numeric: "auto", style: "short" });
    var a = Math.abs(diff);
    if (a < 45) return rtf.format(0, "second");
    if (a < 3600) return rtf.format(Math.round(diff / 60), "minute");
    if (a < 86400) return rtf.format(Math.round(diff / 3600), "hour");
    if (a < 86400 * 7) return rtf.format(Math.round(diff / 86400), "day");
    return new Intl.DateTimeFormat(window.i18n.locale(), { day: "numeric", month: "short", year: "numeric" }).format(new Date(sec * 1000));
  }
  function loadHistory() {
    api("/api/history").then(function (r) {
      if (r.status === 401) { sessionEnded(); return; }
      if (!r.ok) throw new Error();
      S.history = Array.isArray(r.data) ? r.data : [];
      renderHistory(true);
    }).catch(function () {
      $("hist-empty").hidden = false;
      $("hist-empty").textContent = t("hist.load_fail");
    });
  }
  function histFocusMark() {
    var list = $("hist-list"), ae = document.activeElement;
    if (!ae || !list.contains(ae)) return null;
    var li = ae.closest(".hist-item");
    var items = Array.prototype.slice.call(list.querySelectorAll(".hist-item"));
    return { at: Math.max(0, items.indexOf(li)), del: !!ae.closest(".hist-del"), id: li ? +li.getAttribute("data-id") : null };
  }
  function histFocusRestore(mark) {
    if (!mark) return;
    var ae = document.activeElement;
    if (ae && ae !== document.body && document.contains(ae)) return;
    var list = $("hist-list");
    var same = mark.id != null ? list.querySelector('.hist-item[data-id="' + mark.id + '"]') : null;
    var items = list.querySelectorAll(".hist-item");
    var li = same || items[Math.min(mark.at, items.length - 1)];
    var target = li ? li.querySelector(mark.del && same ? ".hist-del" : ".hist-open") : null;
    if (target) { target.focus(); return; }
    var link = $("hist-empty").querySelector("a");
    if (link && !$("hist-empty").hidden) { link.focus(); return; }
    if ($("hist-search").offsetParent !== null) $("hist-search").focus();
  }
  function renderHistory(keepFocus) {
    var mark = keepFocus ? histFocusMark() : null;
    var res = renderHistoryRows();
    histFocusRestore(mark);
    return res;
  }
  function renderHistoryRows() {
    var q = S.query.trim().toLowerCase();
    var rows = S.history.filter(function (r) {
      if (S.pendingDeletes.has(r.id)) return false;
      var st = FILTER_OF[histStatus(r)] || histStatus(r);
      if (S.filter !== "all" && st !== S.filter) return false;
      if (!q) return true;
      return (histTitle(r) + " " + (r.input || "")).toLowerCase().indexOf(q) >= 0;
    });
    var list = $("hist-list");
    list.innerHTML = rows.map(function (r) {
      var st = histStatus(r), title = histTitle(r);
      return '<li class="hist-item' + (S.activeHistory === r.id ? " is-active" : "") + '" data-id="' + r.id + '">' +
        '<button type="button" class="hist-open" data-open="' + r.id + '"' + (S.activeHistory === r.id ? ' aria-current="true"' : "") + ">" +
        '<span class="hist-title" title="' + esc(title) + '">' + histTitleHtml(r) + "</span>" +
        '<span class="hist-meta"><span class="chip tone-' + TONE[st] + '">' + icons[ICON[st]] + esc(t("status." + st)) + "</span>" +
        '<time datetime="' + new Date(r.created_at * 1000).toISOString() + '" title="' + esc(relTime(r.created_at)) + '">' + esc(relTime(r.created_at)) + "</time></span></button>" +
        '<button type="button" class="icon-btn hist-del" data-del="' + r.id + '" aria-label="' + esc(t("hist.delete", { title: title })) + '" title="' + esc(t("hist.delete", { title: title })) + '">' + icons.trash + "</button></li>";
    }).join("");
    var empty = $("hist-empty");
    var live = S.history.filter(function (r) { return !S.pendingDeletes.has(r.id); }).length;
    if (!live) { empty.hidden = false; empty.textContent = t("hist.empty"); }
    else if (!rows.length) { empty.hidden = false; empty.textContent = t(q ? "hist.none_match" : "hist.none_filter"); }
    else empty.hidden = true;
    return { rows: rows.length, live: live, query: q };
  }
  var histAnnounceTimer = null;
  function announceHistory(res) {
    clearTimeout(histAnnounceTimer);
    histAnnounceTimer = setTimeout(function () {
      if (!res.live) return;
      announceStatus(res.rows ? tp("hist.count", res.rows) : t(res.query ? "hist.none_match" : "hist.none_filter"));
    }, 500);
  }
  function wireHistory() {
    $("hist-search").addEventListener("input", function (e) { S.query = e.target.value; announceHistory(renderHistory()); });
    document.querySelectorAll("#hist-filters .filter").forEach(function (b) {
      b.addEventListener("click", function () {
        S.filter = b.getAttribute("data-filter");
        document.querySelectorAll("#hist-filters .filter").forEach(function (x) { x.setAttribute("aria-pressed", x === b ? "true" : "false"); });
        announceHistory(renderHistory());
      });
    });
    $("hist-list").addEventListener("click", function (e) {
      var d = e.target.closest("[data-del]");
      if (d) { deleteHistory(+d.getAttribute("data-del")); return; }
      var o = e.target.closest("[data-open]");
      if (o) openHistory(+o.getAttribute("data-open"));
    });
    window.addEventListener("pagehide", flushDeletes);
    if (!(typeof Request !== "undefined" && "keepalive" in Request.prototype)) {
      document.addEventListener("visibilitychange", function () { if (document.hidden) flushDeletes(); });
    }
    $("rail-toggle").innerHTML = icons.history;
    $("rail-close").innerHTML = icons.close;
    $("rail-toggle").addEventListener("click", function () { setDrawer(!document.body.classList.contains("drawer-open")); });
    $("rail-close").addEventListener("click", function () { setDrawer(false, true); });
    $("rail-scrim").addEventListener("click", function () { setDrawer(false); });
    var wide = matchMedia("(min-width: 1200px)");
    var onWide = function () { if (wide.matches && document.body.classList.contains("drawer-open")) setDrawer(false); };
    if (wide.addEventListener) wide.addEventListener("change", onWide); else if (wide.addListener) wide.addListener(onWide);
    document.addEventListener("keydown", function (e) {
      if (e.key === "Escape" && !e.defaultPrevented && document.body.classList.contains("drawer-open")) { e.preventDefault(); setDrawer(false, true); }
    });
  }
  function closeDrawerByBack() { setDrawer(false, true); }
  function setDrawer(open, focusToggle) {
    var was = document.body.classList.contains("drawer-open");
    document.body.classList.toggle("drawer-open", open);
    $("rail-toggle").setAttribute("aria-expanded", open ? "true" : "false");
    $("rail-scrim").hidden = !open;
    if (open !== was) {
      if (open) GS.backLayer(closeDrawerByBack); else GS.dropLayer(closeDrawerByBack);
      GS.modal($("rail"), open, t("hist.title"));
      GS.lockScroll(open);
    }
    if (open) setTimeout(function () { (coarse() ? $("rail-close") : $("hist-search")).focus({ preventScroll: true }); }, 30);
    else if (focusToggle) $("rail-toggle").focus({ preventScroll: true });
  }
  function untitledEnd(s) { return String(s).replace(/[.!?…]+$/, ""); }
  function deleteHistory(id) {
    var row = S.history.filter(function (r) { return r.id === id; })[0];
    if (!row) return;
    var hadFocus = document.activeElement && document.activeElement.closest && document.activeElement.closest(".hist-item");
    var visible = Array.prototype.slice.call($("hist-list").querySelectorAll(".hist-item"));
    var at = visible.findIndex(function (li) { return +li.getAttribute("data-id") === id; });
    var timer = setTimeout(function () { commitDelete(id); }, 6000);
    S.pendingDeletes.set(id, timer);
    renderHistory();
    var tst = null;
    var undo = function () {
      if (!S.pendingDeletes.has(id)) return;
      var ae = document.activeElement;
      var refocus = !ae || ae === document.body || $("rail").contains(ae) || !!(tst && tst.el.contains(ae));
      clearTimeout(S.pendingDeletes.get(id)); S.pendingDeletes.delete(id); renderHistory();
      var back = $("hist-list").querySelector('[data-open="' + id + '"]');
      if (refocus) {
        if (back) back.focus();
        else if ($("hist-search").offsetParent !== null) $("hist-search").focus();
      }
      announceStatus(t("hist.restored", { title: untitledEnd(histTitle(row)) }));
    };
    tst = GS.toast(t(coarse() ? "hist.deleted.touch" : "hist.deleted", { title: untitledEnd(histTitle(row)), keys: undoKeys() }), { ms: 6000, action: t("undo"), onAction: undo });
    var input = null;
    if (hadFocus) {
      var rows = $("hist-list").querySelectorAll(".hist-open");
      var next = rows[Math.min(Math.max(at, 0), rows.length - 1)];
      if (!next) input = $("hist-search");
      (next || $("hist-search")).focus();
    }
    if (tst && tst.button && !coarse()) tst.button.setAttribute("aria-keyshortcuts", isMac ? "Meta+Z" : "Control+Z");
    S.lastUndo = { run: undo, toast: tst, until: Date.now() + 6000, input: input, inputValue: input ? input.value : null };
  }
  function commitDelete(id, keepalive) {
    if (!S.pendingDeletes.has(id)) return;
    clearTimeout(S.pendingDeletes.get(id));
    S.pendingDeletes.delete(id);
    api("/api/history/" + id, { method: "DELETE", keepalive: !!keepalive }).then(function (r) {
      if (r.status === 401) { sessionEnded(); GS.toast(t("hist.session_ended")); return; }
      if (!r.ok && r.status !== 404) { GS.toast(t("hist.del_fail")); loadHistory(); return; }
      S.history = S.history.filter(function (x) { return x.id !== id; });
      renderHistory(true);
    }).catch(function () {});
  }
  function flushDeletes() { Array.from(S.pendingDeletes.keys()).forEach(function (id) { commitDelete(id, true); }); }
  function openHistory(id) {
    var row = S.history.filter(function (r) { return r.id === id; })[0];
    if (!row) return;
    var draft = editor.get(), wasBusy = S.busy;
    var dirty = !!draft.trim() && draft.trim() !== (S.baseline || "").trim() && draft.trim() !== String(row.input || "").trim() && !exampleOf(draft);
    if (S.busy) { S.quietCancel = true; cancel(); }
    if (S.refining) stopRefining();
    if (document.body.classList.contains("drawer-open")) setDrawer(false);
    var seq = S.openSeq = (S.openSeq || 0) + 1;
    S.lastError = null;
    clearResult();
    show("state-empty");
    function adopt() {
      S.activeHistory = id;
      renderHistory();
      setMode("geo");
      editor.set(row.input);
      S.baseline = row.input;
      if (dirty || wasBusy) offerDraftBack(draft, row, wasBusy);
    }
    function gone(msg) {
      S.activeHistory = null;
      renderHistory(true);
      GS.toast(msg);
    }
    api("/api/history/" + id).then(function (r) {
      if (seq !== S.openSeq) return;
      if (r.ok && validSolution(r.data)) { adopt(); r.data.history_id = id; renderSolution(r.data, { announce: true, focus: true }); return; }
      if (r.ok) { S.activeHistory = null; renderHistory(); showError(unreadable()); return; }
      if (r.status === 410) { adopt(); solveReplay(row); return; }
      if (r.status === 401) { S.activeHistory = null; showError(httpError(r), true); return; }
      if (r.status === 404) {
        S.history = S.history.filter(function (x) { return x.id !== id; });
        gone(t("hist.gone"));
        return;
      }
      gone(t("hist.open_fail"));
    }).catch(function () { if (seq === S.openSeq) gone(t("hist.open_fail")); });
  }
  function offerDraftBack(draft, row, wasBusy) {
    var ta = $("geo-input");
    var restore = function () {
      setMode("geo");
      editor.set(draft);
      S.baseline = null;
      if (document.activeElement !== ta) ta.focus();
    };
    var key = (wasBusy ? "hist.replaced_solve" : "hist.replaced") + (coarse() ? ".touch" : "");
    var tst = GS.toast(t(key, { title: untitledEnd(histTitle(row)), keys: undoKeys() }), { ms: 8000, action: t("undo"), onAction: restore });
    if (tst && tst.button && !coarse()) tst.button.setAttribute("aria-keyshortcuts", isMac ? "Meta+Z" : "Control+Z");
    S.lastUndo = { run: restore, toast: tst, until: Date.now() + 8000, input: ta, inputValue: row.input };
  }
  function solveReplay(row) {
    clearResult();
    show("state-solving");
    paintStepper(["solve"], 0);
    $("solving-title").textContent = t("solving.solving");
    startTimer(S.deadline);
    setBusy(true);
    var ctl = new AbortController();
    S.abort = ctl;
    api("/api/solve", { method: "POST", body: { input: row.input, title: row.title, record: false }, signal: ctl.signal }).then(function (r) {
      stopTimer(); setBusy(false); S.abort = null;
      if (!r.ok) { showError(httpError(r, "solve")); return; }
      if (!validSolution(r.data)) { showError(unreadable()); return; }
      renderSolution(r.data, { announce: true, focus: true });
    }).catch(function (e) {
      var ae = document.activeElement, lost = !ae || ae === document.body || $("state-solving").contains(ae);
      var quiet = S.quietCancel;
      S.quietCancel = false;
      stopTimer(); setBusy(false); S.abort = null;
      if (e && e.name === "AbortError") { if (quiet) return; show("state-empty"); if (lost) $("solve").focus(); return; }
      showError(networkError(e), lost);
    });
  }

  // ------------------------------------------------------------ the figure --
  var viewer = new GS.Viewer({
    frame: $("fig-frame"), viewport: $("fig-viewport"), zoomIn: $("z-in"), zoomOut: $("z-out"),
    fit: $("z-fit"), full: $("z-full"), label: $("zoom-label"),
  });
  if (window.ResizeObserver) new ResizeObserver(function () { requestAnimationFrame(paintFigHint); }).observe($("fig-viewport"));
  viewer.onPoint = function (name) { return stepsApi ? stepsApi.markPoint(name) : []; };
  viewer.onPointAnnounce = function (name, used) {
    announce(used.length ? t("fig.point", { p: name, steps: used.join(", ") }) : t("fig.point.none", { p: name }));
  };

  var peek = { el: null, big: false, figSeen: true, lit: false, timer: 0 };
  function peekWanted() {
    return peek.lit && !peek.figSeen && !!viewer.svg && !viewer.isFull() && matchMedia("(max-width: 1023px)").matches && !document.body.classList.contains("drawer-open");
  }
  function paintPeek() {
    var el = peek.el;
    if (!peekWanted()) {
      if (el && !el.hidden) { el.hidden = true; el.textContent = ""; peek.big = false; el.classList.remove("is-big"); }
      return;
    }
    if (!el) {
      el = peek.el = document.createElement("div");
      el.className = "fig-peek";
      el.id = "fig-peek";
      el.setAttribute("aria-hidden", "true");
      el.hidden = true;
      el.addEventListener("pointerdown", function (e) { e.preventDefault(); });
      el.addEventListener("mousedown", function (e) { e.preventDefault(); });
      el.addEventListener("pointerup", function () { peek.big = !peek.big; paintPeek(); });
      document.body.appendChild(el);
    }
    var b0 = viewer.base0 || viewer.base;
    var ratio = Math.max(0.6, Math.min(1.25, b0.h / b0.w));
    var W = document.documentElement.clientWidth, H = window.innerHeight;
    var ae = document.activeElement, ar = ae && ae !== document.body && ae.getBoundingClientRect ? ae.getBoundingClientRect() : null;
    var header = document.querySelector(".site-header"), hr = header.getBoundingClientRect();
    var top = getComputedStyle(header).position === "sticky" ? Math.max(0, hr.bottom) : 0;
    var atTop = !!ar && (ar.top + ar.bottom) / 2 > (top + H) / 2;
    var small = Math.min(176, W * 0.42, (H * 0.36) / ratio);
    var w = small;
    if (peek.big) {
      var room = ar ? (atTop ? ar.top - top - 24 : H - ar.bottom - 24) : H * 0.62;
      w = Math.min(360, W - 32, (H * 0.62) / ratio, Math.max(small * 1.4, room / ratio));
    }
    w = Math.max(96, Math.round(w));
    var h = Math.round(w * ratio);
    var s0 = Math.min((w - 2) / b0.w, (h - 2) / b0.h);
    var pad = Math.max(0, (Math.max(0.35, Math.min(4.5, 15 / s0 / 18)) - 1) * 32);
    var b = { x: b0.x - pad, y: b0.y - pad, w: b0.w + 2 * pad, h: b0.h + 2 * pad };
    el.style.setProperty("--peek-w", w + "px");
    el.style.setProperty("--peek-h", h + "px");
    el.classList.toggle("is-big", peek.big);
    el.style.setProperty("--peek-top", Math.round(top + 12) + "px");
    el.classList.toggle("at-top", atTop);
    el.classList.toggle("at-bottom", !atTop);
    var svg = viewer.svg.cloneNode(true);
    svg.removeAttribute("role");
    svg.removeAttribute("aria-label");
    svg.setAttribute("aria-hidden", "true");
    svg.setAttribute("focusable", "false");
    svg.setAttribute("viewBox", b.x + " " + b.y + " " + b.w + " " + b.h);
    el.textContent = "";
    el.appendChild(svg);
    el.hidden = false;
    GS.fitLabels(svg, b, { width: w - 2, height: h - 2 });
  }
  viewer.onHighlight = function (on) {
    peek.lit = on;
    clearTimeout(peek.timer);
    peek.timer = setTimeout(paintPeek, on ? 0 : 120);
  };
  if (window.IntersectionObserver) {
    new IntersectionObserver(function (entries) {
      var e = entries[entries.length - 1];
      peek.figSeen = e.isIntersecting && e.intersectionRatio >= 0.5;
      paintPeek();
    }, { threshold: [0, 0.5, 1] }).observe($("fig-viewport"));
  }
  window.addEventListener("resize", function () { if (peek.el && !peek.el.hidden) paintPeek(); });

  // ------------------------------------------------------------------ init --
  function wire() {
    paintIcons();
    setIcon("z-in", "plus"); setIcon("z-out", "minusSm"); setIcon("z-fit", "fit"); setIcon("z-full", "expand"); setIcon("z-svg", "download");
    $("solve-kbd").textContent = isMac ? "⌘ ↵" : "Ctrl ↵";
    $("solve").setAttribute("title", t("solve.shortcut", { keys: isMac ? "⌘ Enter" : "Ctrl Enter" }));
    paintFigHint();
    wireTabs();
    wireEffort();
    wirePhoto();
    wireExamples();
    wireHistory();
    paintExamples();
    $("solve").addEventListener("click", solve);
    ["ai-pill", "ai-pill-m"].forEach(function (id) {
      $(id).addEventListener("click", function () { if (coarse() && $(id).title) GS.toast($(id).title, { ms: 6000 }); });
    });
    $("cancel").addEventListener("click", cancel);
    $("cancel-2").addEventListener("click", cancel);
    $("clear").addEventListener("click", function () {
      var before = { geo: editor.get(), text: $("describe-input").value, mode: S.mode, photo: S.photo };
      if (!clearable()) return;
      var input = null;
      if (S.mode === "geo") { editor.set(""); input = $("geo-input"); }
      else if (S.mode === "describe") { $("describe-input").value = ""; store.del("gs.draft.describe"); input = $("describe-input"); }
      else setPhoto(null);
      var restore = function () {
        editor.set(before.geo);
        $("describe-input").value = before.text;
        if (before.text) store.set("gs.draft.describe", before.text);
        if (before.mode === "photo" && before.photo && !S.photo && !S.photoPending) {
          S.photo = before.photo;
          paintPhoto(before.photo);
          paintSolveEnabled();
        }
        if (input && document.activeElement !== input) input.focus();
      };
      var tst = GS.toast(t(coarse() ? "cleared.touch" : "cleared", { keys: undoKeys() }), { action: t("undo"), ms: 6000, onAction: restore });
      if (tst && tst.button && !coarse()) tst.button.setAttribute("aria-keyshortcuts", isMac ? "Meta+Z" : "Control+Z");
      S.lastUndo = { run: restore, toast: tst, until: Date.now() + 6000, input: input, inputValue: "" };
      if (input) input.focus();
    });
    $("z-svg").addEventListener("click", function () { if (S.sol) exportAs("svg"); });
    $("ptab-steps").addEventListener("click", function () { selectProofTab("steps"); });
    $("ptab-ai").addEventListener("click", function () { selectProofTab("ai"); });
    [$("ptab-steps"), $("ptab-ai")].forEach(function (b) {
      b.addEventListener("keydown", function (e) {
        if (e.key === "ArrowRight" || e.key === "ArrowLeft") { e.preventDefault(); var to = b.id === "ptab-steps" ? "ai" : "steps"; selectProofTab(to); $("ptab-" + to).focus(); }
      });
    });
    $("ai-regen").addEventListener("click", function () { loadAi(true); });
    $("logout").addEventListener("click", function () {
      flushDeletes();
      store.del("gs.draft.geo"); store.del("gs.draft.describe");
      api("/api/auth/logout", { method: "POST" }).then(function () { location.href = "/"; }, function () { location.href = "/"; });
    });
    var d = $("describe-input");
    d.addEventListener("input", function () { store.set("gs.draft.describe", d.value); });
    document.addEventListener("keydown", function (e) {
      var u0 = S.lastUndo;
      var inField = e.target.closest && e.target.closest("textarea, input");
      var fieldOk = !inField || (u0 && u0.input === e.target && e.target.value === u0.inputValue);
      if ((e.key === "z" || e.key === "Z") && (e.ctrlKey || e.metaKey) && !e.shiftKey && u0 && Date.now() < u0.until && fieldOk) {
        e.preventDefault();
        var u = S.lastUndo; S.lastUndo = null;
        if (u.toast) u.toast.close();
        u.run();
        return;
      }
      if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        if (e.target.closest && e.target.closest("#composer")) { e.preventDefault(); solve(); }
      }
      if (e.key === "Escape" && S.busy && !e.defaultPrevented && !menuOpen && !viewer.isFull() &&
          !document.body.classList.contains("drawer-open") && !document.querySelector(".verdict .menu:not([hidden])")) cancel();
    });
    var draft = store.get("gs.draft.geo");
    editor.set(draft != null ? draft : exampleSrc(EXAMPLES[0]));
    var exDraft = exampleOf(editor.get());
    if (exDraft && editor.get() !== exampleSrc(exDraft)) editor.set(exampleSrc(exDraft));
    S.baseline = exDraft ? editor.get() : null;
    figTools(false);
    $("hl").tabIndex = -1;
    var dd = store.get("gs.draft.describe");
    if (dd) d.value = dd;
    setMode("geo", false, "boot");
    show("state-empty");
    GS.initTheme();
  }

  document.addEventListener("langchange", function () {
    clearTimeout(announceTimer); clearTimeout(statusTimer);
    $("announce").textContent = ""; $("announce-status").textContent = "";
    paintAiPill(); paintGates(); paintEffortHint(); paintExamples(); paintAccount();
    if (S.photo) $("photo-meta").textContent = t("photo.meta", { w: S.photo.w, h: S.photo.h, size: fmtBytes(S.photo.bytes) });
    var ex = exampleOf(editor.get());
    if (ex && editor.get() !== exampleSrc(ex)) { editor.set(exampleSrc(ex)); S.baseline = exampleSrc(ex); }
    if (S.busy) paintStage();
    if (S.lastError && !$("state-error").hidden) {
      if (S.lastError.compile && S.geo) {
        api("/api/solve", { method: "POST", body: { input: S.geo, record: false } }).then(function (r) {
          if (r.status === 400 && r.data.code === "compile") showError(httpError(r));
        });
      } else showError(S.lastError);
    }
    if (S.lastFieldErr) fieldError(S.lastFieldErr, t(S.lastFieldErr === "describe-err" ? "err.empty_describe" : S.photoErrKey || "err.empty_photo"));
    paintFigHint();
    $("solve").setAttribute("title", t("solve.shortcut", { keys: isMac ? "⌘ Enter" : "Ctrl Enter" }));
    paintDocTitle();
    if (S.history.length) renderHistory();
    if (S.sol) {
      var tab = $("ptab-ai").getAttribute("aria-selected") === "true" ? "ai" : "steps";
      renderVerdict(S.sol); renderStatement(S.sol); renderProof(S.sol); renderDetails(S.sol);
      viewer.svg && viewer.svg.setAttribute("aria-label", figureAria(S.sol));
      if (tab === "ai" && !$("proof-tabs").hidden) selectProofTab("ai");
    }
  });

  window.i18n.apply();
  wire();
  loadStatus(0);
  GS.booted = true;
})();
