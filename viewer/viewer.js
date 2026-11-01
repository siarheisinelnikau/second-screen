"use strict";
const $ = id => document.getElementById(id);
const catalogs = typeof SECOND_SCREEN_LOCALES === "undefined" ? {} : SECOND_SCREEN_LOCALES;
function preference(name, fallback) { try { return localStorage.getItem(`SecondScreen.${name}`) || fallback; } catch (_) { return fallback; } }
let language = preference("language", "en"), theme = preference("theme", "dark");
if (!["en", "ru", "fr", "de", "es", "it"].includes(language)) language = "en";
if (!["light", "dark"].includes(theme)) theme = "dark";
const t = (key, values = {}) => Object.entries(values).reduce((text, [name, value]) => text.replaceAll(`{${name}}`, String(value)), catalogs[language]?.[key] || key);
let token = null, running = false, controller = null, objectUrl = null, state = null, frameSize = null;
let status = { key: "Disconnected", values: {} }, errorDetail = "";
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
const sourceKey = () => state?.source === "Synthetic" ? "Synthetic test pattern" : state?.source === "VirtualDisplay" ? "Virtual display (experimental)" : "Primary monitor";
const modeKey = () => state?.source === "VirtualDisplay" ? "Virtual display (experimental)" : state?.source === "Synthetic" ? "Synthetic test pattern" : "Mirroring";
function renderState() {
  $("status").textContent = t(status.key, status.key === "Connected · {mode}" ? { mode: t(modeKey()) } : status.values);
  $("mode").textContent = state ? t(modeKey()) : t("VIEWER");
  $("waiting").textContent = !state?.approved ? t("Waiting for approval on your computer…") : state.source === "VirtualDisplay" ? t("Waiting for virtual desktop output. Enable Extend in Windows Display settings.") : t("Waiting for the first desktop frame…");
  $("error").textContent = errorDetail ? t("Connection failed: {detail}", { detail: t(errorDetail) }) : "";
  $("screen").alt = t("{source} desktop", { source: t(sourceKey()) });
  $("details").textContent = frameSize ? t("{width} × {height} · {source} · read only", { ...frameSize, source: t(sourceKey()) }) : t("Mirroring · read only");
}
function setStatus(key, values = {}) { status = { key, values }; renderState(); }
function applyPreferences(save = false) {
  if (document.documentElement) { document.documentElement.lang = language; document.documentElement.dataset.theme = theme; }
  for (const element of document.querySelectorAll?.("[data-i18n]") || []) element.textContent = t(element.dataset.i18n);
  $("language").value = language; $("theme").value = theme;
  $("code").placeholder = t("10-character code");
  const version = document.querySelector?.('meta[name="application-version"]')?.content || "0.3.0";
  $("footer").textContent = t("SecondScreen {version} · One paired viewer · No touch or keyboard forwarding", { version });
  document.title = `SecondScreen · ${t("VIEWER")}`;
  renderState();
  if (save) { try { localStorage.setItem("SecondScreen.language", language); localStorage.setItem("SecondScreen.theme", theme); } catch (_) { /* Preferences remain usable when browser storage is unavailable. */ } }
}
$("language").addEventListener("change", () => { if (["en", "ru", "fr", "de", "es", "it"].includes($("language").value)) language = $("language").value; applyPreferences(true); });
$("theme").addEventListener("change", () => { theme = $("theme").value === "light" ? "light" : "dark"; applyPreferences(true); });
applyPreferences();
function clearFrame() {
  $("screen").removeAttribute("src");
  if (objectUrl) URL.revokeObjectURL(objectUrl);
  objectUrl = null;
}
function reset() {
  running = false;
  if (controller) controller.abort();
  token = null; state = null; frameSize = null;
  clearFrame();
  $("pairing").hidden = false; $("display").hidden = true;
  $("connect").disabled = false;
  setStatus("Disconnected");
}
async function api(path, options = {}) {
  const headers = { ...options.headers };
  if (token) headers.Authorization = `Bearer ${token}`;
  const result = await fetch(path, { ...options, headers, cache: "no-store", signal: controller.signal });
  if (!result.ok) throw new Error((await result.text()).slice(0, 300) || `HTTP ${result.status}`);
  return result;
}
$("pair-form").addEventListener("submit", async event => {
  event.preventDefault();
  $("connect").disabled = true; errorDetail = ""; renderState();
  controller = new AbortController();
  try {
    const response = await api("/api/pair", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ code: $("code").value.trim().toUpperCase() }) });
    token = (await response.json()).token;
    $("code").value = "";
    $("pairing").hidden = true; $("display").hidden = false; $("waiting").hidden = false;
    setStatus("Waiting for host approval");
    running = true;
    await stream();
  } catch (error) {
    if (error.name !== "AbortError") { errorDetail = error.message; reset(); }
  } finally { $("connect").disabled = false; }
});
async function stream() {
  let after = 0, nextState = 0;
  while (running) {
    const started = performance.now();
    if (started >= nextState) {
      state = await (await api("/api/state")).json();
      nextState = started + 1000;
      if (state.error) throw new Error(state.error);
      if (state.hasFrame === false) { clearFrame(); frameSize = null; $("waiting").hidden = false; }
      renderState();
    }
    if (!state.approved) { await delay(500); continue; }
    const response = await api(`/api/frame?after=${after}`);
    if (response.status === 200) {
      const blob = await response.blob();
      if (blob.size > 8 * 1024 * 1024) throw new Error("Frame exceeds the viewer limit");
      const url = URL.createObjectURL(blob);
      // Decode before requesting another frame. A slow tablet cannot build a backlog.
      const image = new Image(); image.src = url;
      try { await image.decode(); } catch (error) { URL.revokeObjectURL(url); throw error; }
      if (!running) { URL.revokeObjectURL(url); break; }
      clearFrame(); objectUrl = url; $("screen").src = url;
      after = Number(response.headers.get("X-Frame-Sequence"));
      frameSize = { width: image.naturalWidth, height: image.naturalHeight };
      $("waiting").hidden = true;
      setStatus("Connected · {mode}");
    }
    await delay(Math.max(0, 1000 / state.fps - (performance.now() - started)));
  }
}
$("disconnect").addEventListener("click", async () => {
  // Allow the revoke request to finish before aborting the remaining fetches.
  running = false;
  try { await api("/api/disconnect", { method: "POST" }); } catch (_) { /* Idle expiry also revokes access. */ }
  errorDetail = ""; reset();
});
$("fullscreen").addEventListener("click", async () => {
  try { await $("stage").requestFullscreen(); } catch (_) { setStatus("Full screen unavailable in this browser"); }
});
document.addEventListener("visibilitychange", () => { if (document.hidden && token) setStatus("Background tab · may disconnect"); });
window.addEventListener("pagehide", () => { running = false; clearFrame(); });
