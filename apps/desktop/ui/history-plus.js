"use strict";

(() => {
  const invoke = window.__TAURI__?.core?.invoke;
  const listen = window.__TAURI__?.event?.listen;
  const section = document.querySelector('[data-view="history"]');
  const legacyMessage = document.getElementById("history-message");
  const legacyList = document.getElementById("history-list");
  const refreshButton = document.getElementById("refresh-history");
  const historyNav = document.querySelector('[data-view-target="history"]');
  const panel = section?.querySelector(".feature-panel");
  if (!invoke || !section || !panel || !refreshButton) return;

  const toolbar = document.createElement("div");
  toolbar.className = "history-plus-toolbar";
  toolbar.innerHTML = `
    <label class="history-plus-search-wrap">
      <span class="sr-only">Search transcripts</span>
      <input id="history-plus-search" type="search" autocomplete="off" placeholder="Search transcripts, model or language…" />
    </label>
    <select id="history-plus-filter" aria-label="Filter transcript history">
      <option value="all">All transcripts</option>
      <option value="submitted">Submitted</option>
      <option value="failed">Insertion failed</option>
      <option value="shortcut">Shortcut</option>
      <option value="desktop">Desktop UI</option>
    </select>
  `;
  section.insertBefore(toolbar, panel);

  const message = document.createElement("div");
  message.id = "history-plus-message";
  message.className = "message muted";
  message.setAttribute("role", "status");
  const list = document.createElement("div");
  list.id = "history-plus-list";
  list.className = "history-plus-list";
  list.setAttribute("aria-live", "polite");
  panel.append(message, list);

  if (legacyMessage) legacyMessage.hidden = true;
  if (legacyList) legacyList.hidden = true;

  const search = document.getElementById("history-plus-search");
  const filter = document.getElementById("history-plus-filter");
  const state = { entries: [], busy: false, unlisten: null };

  function errorMessage(error) {
    if (typeof error === "string") return error;
    if (error && typeof error.message === "string") return error.message;
    return "Local history is unavailable.";
  }

  function deliveryLabel(value) {
    switch (value) {
      case "deliveredVerified": return "Delivery verified";
      case "backendSubmittedUnverified": return "Submitted";
      case "insertionFailed": return "Insertion failed";
      case "transcribedOnly": return "Transcribed only";
      default: return value || "Unknown state";
    }
  }

  function deliveryKind(value) {
    if (value === "deliveredVerified") return "success";
    if (value === "backendSubmittedUnverified") return "submitted";
    if (value === "insertionFailed") return "failed";
    return "neutral";
  }

  function startOfDay(date) {
    return new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  }

  function groupLabel(timestampMs) {
    const date = new Date(timestampMs);
    const now = new Date();
    const delta = Math.round((startOfDay(now) - startOfDay(date)) / 86_400_000);
    if (delta === 0) return "Today";
    if (delta === 1) return "Yesterday";
    return date.toLocaleDateString(undefined, {
      weekday: "short",
      year: date.getFullYear() === now.getFullYear() ? undefined : "numeric",
      month: "short",
      day: "numeric",
    });
  }

  function matchesFilter(entry) {
    const value = filter.value;
    if (value === "failed") return entry.deliveryState === "insertionFailed";
    if (value === "submitted") {
      return entry.deliveryState === "backendSubmittedUnverified" || entry.deliveryState === "deliveredVerified";
    }
    if (value === "shortcut") return entry.invocationSource === "shortcut";
    if (value === "desktop") return entry.invocationSource === "desktopUi";
    return true;
  }

  function visibleEntries() {
    const query = search.value.trim().toLocaleLowerCase();
    return state.entries.filter((entry) => {
      if (!matchesFilter(entry)) return false;
      if (!query) return true;
      return [entry.transcript, entry.detectedLanguage, entry.modelId, entry.engineId, entry.insertionBackend]
        .filter(Boolean)
        .some((value) => String(value).toLocaleLowerCase().includes(query));
    });
  }

  function button(label, className, action) {
    const element = document.createElement("button");
    element.type = "button";
    element.className = className;
    element.textContent = label;
    element.addEventListener("click", action);
    return element;
  }

  async function copyText(buttonElement, text) {
    try {
      await navigator.clipboard.writeText(text);
      const previous = buttonElement.textContent;
      buttonElement.textContent = "Copied";
      window.setTimeout(() => { buttonElement.textContent = previous; }, 1100);
    } catch {
      buttonElement.textContent = "Copy unavailable";
    }
  }

  function appendDetails(card, entry) {
    const details = document.createElement("details");
    details.className = "history-plus-details";
    const summary = document.createElement("summary");
    summary.textContent = "Details";
    const metadata = document.createElement("dl");
    const rows = [
      ["Model", entry.modelId || "Unknown"],
      ["Engine", entry.engineId || "Unknown"],
      ["Insertion", entry.insertionBackend || "Not recorded"],
      ["Source", entry.invocationSource === "shortcut" ? "Global shortcut" : "Desktop UI"],
    ];
    for (const [label, value] of rows) {
      const row = document.createElement("div");
      const dt = document.createElement("dt");
      const dd = document.createElement("dd");
      dt.textContent = label;
      dd.textContent = value;
      row.append(dt, dd);
      metadata.append(row);
    }
    details.append(summary, metadata);
    card.append(details);
  }

  function render() {
    list.replaceChildren();
    const entries = visibleEntries();
    if (!state.entries.length) {
      message.textContent = "No local transcripts yet.";
      return;
    }
    message.textContent = entries.length === state.entries.length
      ? `${entries.length} recent local transcript${entries.length === 1 ? "" : "s"}.`
      : `${entries.length} of ${state.entries.length} transcripts match.`;
    if (!entries.length) {
      const empty = document.createElement("p");
      empty.className = "history-plus-empty";
      empty.textContent = "No transcripts match this search or filter.";
      list.append(empty);
      return;
    }

    let currentLabel = null;
    let currentList = null;
    for (const entry of entries) {
      const label = groupLabel(entry.createdAtUnixMs);
      if (label !== currentLabel) {
        currentLabel = label;
        const group = document.createElement("section");
        group.className = "history-plus-group";
        const heading = document.createElement("h2");
        heading.className = "history-plus-group-heading";
        heading.textContent = label;
        currentList = document.createElement("div");
        currentList.className = "history-plus-group-list";
        group.append(heading, currentList);
        list.append(group);
      }

      const card = document.createElement("article");
      card.className = "history-item history-plus-item";
      const transcript = document.createElement("p");
      transcript.className = "history-plus-transcript";
      transcript.textContent = entry.transcript;
      card.append(transcript);

      const meta = document.createElement("div");
      meta.className = "history-plus-summary";
      const time = document.createElement("span");
      time.textContent = new Date(entry.createdAtUnixMs).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
      const source = document.createElement("span");
      source.textContent = entry.invocationSource === "shortcut" ? "Shortcut" : "Desktop UI";
      const language = document.createElement("span");
      language.textContent = entry.detectedLanguage ? entry.detectedLanguage.toUpperCase() : "Language unknown";
      const delivery = document.createElement("span");
      delivery.className = `history-plus-delivery ${deliveryKind(entry.deliveryState)}`;
      delivery.textContent = deliveryLabel(entry.deliveryState);
      meta.append(time, source, language, delivery);
      card.append(meta);
      appendDetails(card, entry);

      const actions = document.createElement("div");
      actions.className = "history-plus-actions";
      actions.append(button("Copy", "button secondary compact", (event) => {
        void copyText(event.currentTarget, entry.transcript);
      }));
      const remove = button("Delete", "button danger compact", async () => {
        if (state.busy) return;
        state.busy = true;
        remove.disabled = true;
        try {
          await invoke("history_delete", { id: entry.id });
          state.busy = false;
          await refresh();
        } catch (error) {
          message.textContent = errorMessage(error);
        } finally {
          state.busy = false;
        }
      });
      actions.append(remove);
      card.append(actions);
      currentList.append(card);
    }
  }

  async function refresh() {
    if (state.busy) return;
    state.busy = true;
    refreshButton.disabled = true;
    try {
      const health = await invoke("history_status");
      if (!health.available) {
        state.entries = [];
        list.replaceChildren();
        message.textContent = health.lastError || "Local history is unavailable.";
        return;
      }
      const entries = await invoke("history_list", { limit: 500 });
      state.entries = Array.isArray(entries) ? entries : [];
      render();
    } catch (error) {
      state.entries = [];
      list.replaceChildren();
      message.textContent = errorMessage(error);
    } finally {
      state.busy = false;
      refreshButton.disabled = false;
    }
  }

  search.addEventListener("input", render);
  filter.addEventListener("change", render);
  refreshButton.addEventListener("click", () => void refresh());
  historyNav?.addEventListener("click", () => void refresh());

  if (listen) {
    void listen("blcvoice://dictation-lifecycle", (event) => {
      if (event.payload?.state === "completed" || event.payload?.state === "failed") void refresh();
    }).then((unlisten) => { state.unlisten = unlisten; }).catch(() => {});
  }
  window.addEventListener("beforeunload", () => {
    if (state.unlisten) state.unlisten();
  });

  void refresh();
})();
