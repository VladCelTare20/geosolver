/* Sign-in / create-account page. */
(function () {
  "use strict";
  var $ = function (id) { return document.getElementById(id); };
  var t = function (k, v) { return window.i18n.t(k, v); };
  var icons = GS.icons, esc = GS.esc;
  var mode = "login", touched = { user: false, pass: false }, busy = false, lastErr = null, serverField = null;
  var FIELD_OF = { user_taken: "user", username_rule: "user", pw_len: "pass" };
  var USER_RE = /^[A-Za-z0-9_-]{3,32}$/;

  function sentence(s) {
    s = String(s || "").trim();
    if (!s) return s;
    s = s.charAt(0).toUpperCase() + s.slice(1);
    return /[.!?…]$/.test(s) ? s : s + ".";
  }

  function fieldMsg(id, kind, text) {
    var el = $(id);
    var cls = "field-msg" + (kind ? " is-" + kind : "");
    var html = text ? (kind === "err" ? icons.alert : kind === "ok" ? icons.check : "") + "<span>" + esc(text) + "</span>" : "";
    if (el.className === cls && el.getAttribute("data-text") === (text || "")) return;
    el.className = cls;
    el.setAttribute("data-text", text || "");
    el.innerHTML = html;
  }
  function formMsg(kind, text) {
    var box = $("form-msg");
    if (!text) { box.innerHTML = ""; lastErr = null; return; }
    var tone = kind === "ok" ? "tone-proved" : "tone-false";
    box.innerHTML = '<div class="banner ' + tone + '" role="' + (kind === "ok" ? "status" : "alert") + '">' + (kind === "ok" ? icons.check : icons.alert) + "<div>" + esc(text) + "</div></div>";
  }

  function validate(show) {
    var u = $("username").value.trim(), p = $("password").value;
    var uOk = USER_RE.test(u), pOk = p.length >= 8 && p.length <= 128;
    var reg = mode === "register";
    if (reg) {
      if (show || touched.user) {
        if (!u) fieldMsg("user-msg", show ? "err" : "", show ? t("auth.err_user") : t("auth.user_hint"));
        else fieldMsg("user-msg", uOk ? "ok" : "err", uOk ? t("auth.user_ok") : t("auth.err_user"));
      } else fieldMsg("user-msg", "", t("auth.user_hint"));
      if (show || touched.pass) fieldMsg("pass-msg", pOk ? "ok" : (p || show ? "err" : ""), pOk ? t("auth.pass_hint") : t(p.length > 128 ? "auth.err.pass_long" : "auth.err_pass"));
      else fieldMsg("pass-msg", "", t("auth.pass_hint"));
      if (serverField === "user") fieldMsg("user-msg", "err", t("auth.err." + lastErr));
      if (serverField === "pass") fieldMsg("pass-msg", "err", t("auth.err." + lastErr));
    } else {
      fieldMsg("user-msg", show && u.length < 3 ? "err" : "", show && u.length < 3 ? t(u ? "auth.err_user" : "auth.need_user") : "");
      fieldMsg("pass-msg", show && !p ? "err" : "", show && !p ? t("auth.need_pass") : "");
    }
    var userBad = reg ? !uOk : u.length < 3;
    var passBad = reg ? !pOk : !p;
    $("username").setAttribute("aria-invalid", (show && userBad) || serverField === "user" ? "true" : "false");
    $("password").setAttribute("aria-invalid", (show && passBad) || serverField === "pass" ? "true" : "false");
    return { ok: !userBad && !passBad, userBad: userBad };
  }

  function setMode(next, keepFocus, keepErr) {
    mode = next;
    var login = mode === "login";
    [["tab-login", login], ["tab-register", !login]].forEach(function (x) {
      $(x[0]).setAttribute("aria-selected", x[1] ? "true" : "false");
      $(x[0]).tabIndex = x[1] ? 0 : -1;
    });
    $("heading").textContent = login ? t("auth.welcome") : t("auth.create_head");
    $("subhead").textContent = login ? t("auth.sub_login") : t("auth.sub_register");
    $("submit").textContent = login ? t("nav.signin") : t("nav.create");
    $("password").autocomplete = login ? "current-password" : "new-password";
    try { history.replaceState(null, "", login ? "/auth" : "/auth?mode=register"); } catch (e) {}
    if (!keepErr || !lastErr) { formMsg(); serverField = null; }
    else showServerErr(lastErr);
    validate(false);
    if (!keepFocus) $("username").focus();
  }

  function paintToggle() {
    var shown = $("password").type === "text";
    $("pw-toggle").innerHTML = shown ? icons.eyeOff : icons.eye;
    var label = t("auth.show");
    $("pw-toggle").setAttribute("aria-label", label);
    $("pw-toggle").setAttribute("title", label);
    $("pw-toggle").setAttribute("aria-pressed", shown ? "true" : "false");
  }
  function showServerErr(code) {
    lastErr = code;
    var text = code === "net" ? t("auth.net_err") : code === "generic" ? t("auth.generic_err") : t("auth.err." + code);
    var box = $("form-msg");
    box.innerHTML = FIELD_OF[code] ? "" : '<div class="banner tone-false" role="alert">' + icons.alert + "<div>" + esc(text) + "</div></div>";
  }
  function clearServerErr() {
    if (!lastErr) return;
    formMsg();
    serverField = null;
    validate(false);
  }

  ["tab-login", "tab-register"].forEach(function (id, i, all) {
    $(id).addEventListener("click", function () { setMode(id === "tab-login" ? "login" : "register"); });
    $(id).addEventListener("keydown", function (e) {
      if (e.key !== "ArrowLeft" && e.key !== "ArrowRight" && e.key !== "Home" && e.key !== "End") return;
      e.preventDefault();
      var other = all[(i + 1) % 2];
      var target = e.key === "Home" ? all[0] : e.key === "End" ? all[1] : other;
      setMode(target === "tab-login" ? "login" : "register", true);
      $(target).focus();
    });
  });
  $("username").addEventListener("input", function () { touched.user = true; clearServerErr(); validate(false); });
  $("password").addEventListener("input", function () { touched.pass = true; clearServerErr(); validate(false); });
  $("pw-toggle").addEventListener("click", function () {
    $("password").type = $("password").type === "password" ? "text" : "password";
    paintToggle();
    $("password").focus();
  });

  $("form").addEventListener("submit", function (e) {
    e.preventDefault();
    if (busy) return;
    clearServerErr();
    var v = validate(true);
    if (!v.ok) { (v.userBad ? $("username") : $("password")).focus(); return; }
    busy = true;
    var btn = $("submit"), label = btn.textContent;
    btn.disabled = true;
    btn.innerHTML = '<span class="spinner" aria-hidden="true"></span><span>' + esc(t("auth.working")) + "</span>";
    formMsg();
    var username = $("username").value.trim();
    fetch("/api/auth/" + (mode === "login" ? "login" : "register"), {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username: username, password: $("password").value }),
    }).then(function (res) {
      return res.json().catch(function () { return {}; }).then(function (data) {
        if (res.ok) {
          formMsg("ok", t("auth.signed_in", { name: data.username || username }));
          setTimeout(function () { location.href = "/app"; }, 350);
          return;
        }
        busy = false; btn.disabled = false; btn.textContent = label;
        var code = data.code && window.i18n.has("auth.err." + data.code) ? data.code : null;
        serverField = code ? FIELD_OF[code] || null : null;
        if (code) showServerErr(code);
        else if (data.error) { lastErr = null; formMsg("err", sentence(data.error)); }
        else showServerErr("generic");
        validate(false);
        (serverField === "user" ? $("username") : $("password")).focus();
      });
    }).catch(function () {
      busy = false; btn.disabled = false; btn.textContent = label;
      showServerErr("net");
    });
  });

  document.addEventListener("langchange", function () {
    $("submit").textContent = mode === "login" ? t("nav.signin") : t("nav.create");
    setMode(mode, true, true);
    paintToggle();
  });
  window.i18n.apply();
  GS.initTheme();
  paintToggle();
  var wanted = new URLSearchParams(location.search).get("mode");
  setMode(wanted === "register" || location.hash === "#register" ? "register" : "login");
})();
