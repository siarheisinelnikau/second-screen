"use strict";
const $ = id => document.getElementById(id);
let token = null, running = false, controller = null, objectUrl = null;
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
function clearFrame() {
  $("screen").removeAttribute("src");
  if (objectUrl) URL.revokeObjectURL(objectUrl);
  objectUrl = null;
}
function reset(message = "Disconnected") {
  running = false;
  if (controller) controller.abort();
  token = null;
  clearFrame();
  $("pairing").hidden = false; $("display").hidden = true;
  $("status").textContent = message; $("connect").disabled = false;
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
  $("connect").disabled = true; $("error").textContent = "";
  controller = new AbortController();
  try {
    const response = await api("/api/pair", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ code: $("code").value.trim().toUpperCase() }) });
    token = (await response.json()).token;
    $("code").value = "";
    $("pairing").hidden = true; $("display").hidden = false; $("waiting").hidden = false;
    $("status").textContent = "Waiting for host approval";
    running = true;
    await stream();
  } catch (error) {
    if (error.name !== "AbortError") { reset("Disconnected"); $("error").textContent = error.message; }
  } finally { $("connect").disabled = false; }
});
async function stream() {
  let after = 0, state = null, nextState = 0;
  while (running) {
    const started = performance.now();
    if (started >= nextState) {
      state = await (await api("/api/state")).json();
      nextState = started + 1000;
      if (state.error) throw new Error(state.error);
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
      $("waiting").hidden = true;
      $("status").textContent = "Connected · Mirroring";
      $("details").textContent = `${image.naturalWidth} × ${image.naturalHeight} · ${state.source === "Synthetic" ? "Synthetic test pattern" : "Primary monitor"} · read only`;
    }
    await delay(Math.max(0, 1000 / state.fps - (performance.now() - started)));
  }
}
$("disconnect").addEventListener("click", async () => {
  // Allow the revoke request to finish before aborting the remaining fetches.
  running = false;
  try { await api("/api/disconnect", { method: "POST" }); } catch (_) { /* Idle expiry also revokes access. */ }
  reset();
});
$("fullscreen").addEventListener("click", async () => {
  try { await $("stage").requestFullscreen(); } catch (_) { $("status").textContent = "Full screen unavailable in this browser"; }
});
document.addEventListener("visibilitychange", () => { if (document.hidden && token) $("status").textContent = "Background tab · may disconnect"; });
window.addEventListener("pagehide", () => { running = false; clearFrame(); });
