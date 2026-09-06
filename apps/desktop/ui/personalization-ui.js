"use strict";

(() => {
  const invoke = window.__TAURI__?.core?.invoke;
  const dictionaryForm = document.getElementById("dictionary-form");
  const dictionaryInput = document.getElementById("dictionary-term");
  const dictionaryList = document.getElementById("dictionary-list");
  const dictionaryMessage = document.getElementById("dictionary-message");
  const replacementForm = document.getElementById("replacement-form");
  const replacementSource = document.getElementById("replacement-source");
  const replacementDestination = document.getElementById("replacement-destination");
  const replacementList = document.getElementById("replacement-list");
  const replacementMessage = document.getElementById("replacement-message");
  const personalizationStatus = document.getElementById("personalization-status");

  if (!dictionaryForm || !replacementForm) return;

  let dictionaryBusy = false;
  let replacementBusy = false;

  function errorMessage(error) {
    if (error && typeof error === "object" && typeof error.message === "string") return error.message;
    if (typeof error === "string") return error;
    return "Personalization could not be updated.";
  }

  function makeDeleteButton(action) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "button danger compact personalization-delete";
    button.textContent = "Delete";
    button.addEventListener("click", action);
    return button;
  }

  function renderDictionary(terms) {
    dictionaryList.replaceChildren();
    if (!terms.length) {
      dictionaryMessage.textContent = "No custom words yet. Add names, brands and terms Whisper often mishears.";
      return;
    }
    dictionaryMessage.textContent = `${terms.length} custom word${terms.length === 1 ? "" : "s"} will be supplied to Whisper as local context.`;
    for (const item of terms) {
      const row = document.createElement("article");
      row.className = "personalization-row";
      const copy = document.createElement("div");
      copy.className = "personalization-row-copy";
      const term = document.createElement("strong");
      term.textContent = item.term;
      const hint = document.createElement("small");
      hint.textContent = "Recognition vocabulary";
      copy.append(term, hint);
      const remove = makeDeleteButton(async () => {
        if (dictionaryBusy) return;
        dictionaryBusy = true;
        remove.disabled = true;
        try {
          await invoke("dictionary_delete", { id: item.id });
          await refreshDictionary();
        } catch (error) {
          dictionaryMessage.textContent = errorMessage(error);
        } finally {
          dictionaryBusy = false;
        }
      });
      row.append(copy, remove);
      dictionaryList.append(row);
    }
  }

  function renderReplacements(rules) {
    replacementList.replaceChildren();
    if (!rules.length) {
      replacementMessage.textContent = "No replacements yet. Add exact corrections for phrases the model consistently gets wrong.";
      return;
    }
    replacementMessage.textContent = `${rules.length} deterministic replacement${rules.length === 1 ? "" : "s"} will run locally before text is inserted.`;
    for (const rule of rules) {
      const row = document.createElement("article");
      row.className = "personalization-row replacement-row";
      const copy = document.createElement("div");
      copy.className = "replacement-copy";
      const source = document.createElement("span");
      source.textContent = rule.source;
      const arrow = document.createElement("span");
      arrow.className = "replacement-arrow";
      arrow.textContent = "→";
      const destination = document.createElement("strong");
      destination.textContent = rule.replacement;
      copy.append(source, arrow, destination);
      const remove = makeDeleteButton(async () => {
        if (replacementBusy) return;
        replacementBusy = true;
        remove.disabled = true;
        try {
          await invoke("replacement_delete", { id: rule.id });
          await refreshReplacements();
        } catch (error) {
          replacementMessage.textContent = errorMessage(error);
        } finally {
          replacementBusy = false;
        }
      });
      row.append(copy, remove);
      replacementList.append(row);
    }
  }

  async function refreshHealth() {
    try {
      const health = await invoke("personalization_status");
      personalizationStatus.textContent = health.available ? "Local · Ready" : "Unavailable";
      personalizationStatus.className = `state-pill ${health.available ? "passed" : "failed"}`;
    } catch {
      personalizationStatus.textContent = "Unavailable";
      personalizationStatus.className = "state-pill failed";
    }
  }

  async function refreshDictionary() {
    if (!invoke) return;
    try {
      const terms = await invoke("dictionary_list", { limit: 500 });
      renderDictionary(Array.isArray(terms) ? terms : []);
    } catch (error) {
      dictionaryList.replaceChildren();
      dictionaryMessage.textContent = errorMessage(error);
    }
  }

  async function refreshReplacements() {
    if (!invoke) return;
    try {
      const rules = await invoke("replacement_list", { limit: 500 });
      renderReplacements(Array.isArray(rules) ? rules : []);
    } catch (error) {
      replacementList.replaceChildren();
      replacementMessage.textContent = errorMessage(error);
    }
  }

  dictionaryForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const term = dictionaryInput.value.trim();
    if (!invoke || dictionaryBusy || !term) return;
    dictionaryBusy = true;
    const submit = dictionaryForm.querySelector("button[type='submit']");
    submit.disabled = true;
    try {
      await invoke("dictionary_add", { term });
      dictionaryInput.value = "";
      await refreshDictionary();
      dictionaryInput.focus();
    } catch (error) {
      dictionaryMessage.textContent = errorMessage(error);
    } finally {
      dictionaryBusy = false;
      submit.disabled = false;
    }
  });

  replacementForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const source = replacementSource.value.trim();
    const replacement = replacementDestination.value.trim();
    if (!invoke || replacementBusy || !source || !replacement) return;
    replacementBusy = true;
    const submit = replacementForm.querySelector("button[type='submit']");
    submit.disabled = true;
    try {
      await invoke("replacement_add", { source, replacement });
      replacementSource.value = "";
      replacementDestination.value = "";
      await refreshReplacements();
      replacementSource.focus();
    } catch (error) {
      replacementMessage.textContent = errorMessage(error);
    } finally {
      replacementBusy = false;
      submit.disabled = false;
    }
  });

  if (!invoke) {
    dictionaryMessage.textContent = "Open this view inside the BLCVoice desktop app.";
    replacementMessage.textContent = "Open this view inside the BLCVoice desktop app.";
    return;
  }

  void Promise.all([refreshHealth(), refreshDictionary(), refreshReplacements()]);
})();
