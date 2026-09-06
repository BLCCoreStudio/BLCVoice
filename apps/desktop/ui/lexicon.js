"use strict";

(() => {
  const invokeCommand = window.__TAURI__?.core?.invoke;
  const elements = {
    search: document.getElementById("lexicon-search"),
    message: document.getElementById("lexicon-message"),
    dictionaryList: document.getElementById("dictionary-list"),
    dictionaryForm: document.getElementById("dictionary-form"),
    dictionaryId: document.getElementById("dictionary-id"),
    dictionaryTerm: document.getElementById("dictionary-term"),
    dictionaryAliases: document.getElementById("dictionary-aliases"),
    dictionaryEnabled: document.getElementById("dictionary-enabled"),
    dictionarySubmit: document.getElementById("dictionary-submit"),
    dictionaryCancel: document.getElementById("dictionary-cancel-edit"),
    replacementList: document.getElementById("replacement-list"),
    replacementForm: document.getElementById("replacement-form"),
    replacementId: document.getElementById("replacement-id"),
    replacementFrom: document.getElementById("replacement-from"),
    replacementTo: document.getElementById("replacement-to"),
    replacementEnabled: document.getElementById("replacement-enabled"),
    replacementWholeWord: document.getElementById("replacement-whole-word"),
    replacementSubmit: document.getElementById("replacement-submit"),
    replacementCancel: document.getElementById("replacement-cancel-edit"),
    refresh: document.getElementById("refresh-lexicon"),
  };

  if (!elements.dictionaryForm || !elements.replacementForm) return;

  const state = {
    dictionary: [],
    replacements: [],
    busy: false,
  };

  function errorMessage(error) {
    if (typeof error === "string") return error;
    if (error && typeof error.message === "string") return error.message;
    return "The local vocabulary operation failed.";
  }

  function setMessage(message, failed = false) {
    elements.message.textContent = message || "";
    elements.message.classList.toggle("error", failed);
  }

  function aliasesFromInput(value) {
    return value
      .split(/[\n,]/u)
      .map((item) => item.trim())
      .filter(Boolean);
  }

  function resetDictionaryForm() {
    elements.dictionaryId.value = "";
    elements.dictionaryTerm.value = "";
    elements.dictionaryAliases.value = "";
    elements.dictionaryEnabled.checked = true;
    elements.dictionarySubmit.textContent = "Add term";
    elements.dictionaryCancel.hidden = true;
  }

  function resetReplacementForm() {
    elements.replacementId.value = "";
    elements.replacementFrom.value = "";
    elements.replacementTo.value = "";
    elements.replacementEnabled.checked = true;
    elements.replacementWholeWord.checked = true;
    elements.replacementSubmit.textContent = "Add replacement";
    elements.replacementCancel.hidden = true;
  }

  function matchesSearch(...values) {
    const query = (elements.search.value || "").trim().toLocaleLowerCase();
    if (!query) return true;
    return values
      .flat()
      .filter((value) => typeof value === "string")
      .some((value) => value.toLocaleLowerCase().includes(query));
  }

  function actionButton(label, className, action) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = className;
    button.textContent = label;
    button.disabled = state.busy;
    button.addEventListener("click", action);
    return button;
  }

  function statusBadge(enabled) {
    const badge = document.createElement("span");
    badge.className = `lexicon-badge ${enabled ? "enabled" : "disabled"}`;
    badge.textContent = enabled ? "Active" : "Paused";
    return badge;
  }

  function renderDictionary() {
    elements.dictionaryList.replaceChildren();
    const entries = state.dictionary.filter((entry) =>
      matchesSearch(entry.term, entry.aliases || []),
    );
    if (!entries.length) {
      const empty = document.createElement("p");
      empty.className = "lexicon-empty";
      empty.textContent = state.dictionary.length
        ? "No dictionary terms match your search."
        : "No custom terms yet. Add names, brands or words the recognizer often misses.";
      elements.dictionaryList.append(empty);
      return;
    }

    for (const entry of entries) {
      const card = document.createElement("article");
      card.className = "lexicon-item";
      const header = document.createElement("div");
      header.className = "lexicon-item-header";
      const title = document.createElement("strong");
      title.textContent = entry.term;
      header.append(title, statusBadge(entry.enabled));
      card.append(header);

      const detail = document.createElement("p");
      detail.textContent = entry.aliases?.length
        ? `Also recognize: ${entry.aliases.join(", ")}`
        : "Canonical spelling only";
      card.append(detail);

      const actions = document.createElement("div");
      actions.className = "lexicon-actions";
      actions.append(
        actionButton("Edit", "button secondary compact", () => {
          elements.dictionaryId.value = String(entry.id);
          elements.dictionaryTerm.value = entry.term;
          elements.dictionaryAliases.value = (entry.aliases || []).join(", ");
          elements.dictionaryEnabled.checked = entry.enabled;
          elements.dictionarySubmit.textContent = "Save term";
          elements.dictionaryCancel.hidden = false;
          elements.dictionaryTerm.focus();
        }),
        actionButton(
          entry.enabled ? "Pause" : "Enable",
          "button secondary compact",
          () => void saveDictionary({ ...entry, enabled: !entry.enabled }),
        ),
        actionButton("Delete", "button danger compact", () => void deleteDictionary(entry.id)),
      );
      card.append(actions);
      elements.dictionaryList.append(card);
    }
  }

  function renderReplacements() {
    elements.replacementList.replaceChildren();
    const rules = state.replacements.filter((rule) =>
      matchesSearch(rule.from, rule.to),
    );
    if (!rules.length) {
      const empty = document.createElement("p");
      empty.className = "lexicon-empty";
      empty.textContent = state.replacements.length
        ? "No replacement rules match your search."
        : "No replacements yet. Add deterministic fixes for recurring recognition mistakes.";
      elements.replacementList.append(empty);
      return;
    }

    for (const rule of rules) {
      const card = document.createElement("article");
      card.className = "lexicon-item replacement-item";
      const header = document.createElement("div");
      header.className = "lexicon-item-header";
      const mapping = document.createElement("div");
      mapping.className = "replacement-mapping";
      const source = document.createElement("code");
      source.textContent = rule.from;
      const arrow = document.createElement("span");
      arrow.textContent = "→";
      const target = document.createElement("code");
      target.textContent = rule.to;
      mapping.append(source, arrow, target);
      header.append(mapping, statusBadge(rule.enabled));
      card.append(header);

      const detail = document.createElement("p");
      detail.textContent = rule.wholeWord ? "Whole words only" : "Replace inside longer text too";
      card.append(detail);

      const actions = document.createElement("div");
      actions.className = "lexicon-actions";
      actions.append(
        actionButton("Edit", "button secondary compact", () => {
          elements.replacementId.value = String(rule.id);
          elements.replacementFrom.value = rule.from;
          elements.replacementTo.value = rule.to;
          elements.replacementEnabled.checked = rule.enabled;
          elements.replacementWholeWord.checked = rule.wholeWord;
          elements.replacementSubmit.textContent = "Save replacement";
          elements.replacementCancel.hidden = false;
          elements.replacementFrom.focus();
        }),
        actionButton(
          rule.enabled ? "Pause" : "Enable",
          "button secondary compact",
          () => void saveReplacement({ ...rule, enabled: !rule.enabled }),
        ),
        actionButton("Delete", "button danger compact", () => void deleteReplacement(rule.id)),
      );
      card.append(actions);
      elements.replacementList.append(card);
    }
  }

  function render() {
    renderDictionary();
    renderReplacements();
  }

  async function refresh() {
    if (!invokeCommand || state.busy) return;
    elements.refresh.disabled = true;
    try {
      const snapshot = await invokeCommand("text_rules_snapshot");
      state.dictionary = Array.isArray(snapshot?.dictionary) ? snapshot.dictionary : [];
      state.replacements = Array.isArray(snapshot?.replacements) ? snapshot.replacements : [];
      setMessage(
        `${state.dictionary.length} custom term${state.dictionary.length === 1 ? "" : "s"} · ${state.replacements.length} replacement${state.replacements.length === 1 ? "" : "s"}`,
      );
      render();
    } catch (error) {
      setMessage(errorMessage(error), true);
    } finally {
      elements.refresh.disabled = false;
    }
  }

  async function saveDictionary(entry) {
    if (!invokeCommand || state.busy) return;
    state.busy = true;
    render();
    try {
      await invokeCommand("dictionary_save", {
        id: entry.id ?? null,
        term: entry.term,
        aliases: entry.aliases || [],
        enabled: entry.enabled ?? true,
      });
      resetDictionaryForm();
      await refresh();
    } catch (error) {
      setMessage(errorMessage(error), true);
    } finally {
      state.busy = false;
      render();
    }
  }

  async function deleteDictionary(id) {
    if (!invokeCommand || state.busy) return;
    state.busy = true;
    try {
      await invokeCommand("dictionary_delete", { id });
      resetDictionaryForm();
      await refresh();
    } catch (error) {
      setMessage(errorMessage(error), true);
    } finally {
      state.busy = false;
      render();
    }
  }

  async function saveReplacement(rule) {
    if (!invokeCommand || state.busy) return;
    state.busy = true;
    render();
    try {
      await invokeCommand("replacement_save", {
        id: rule.id ?? null,
        from: rule.from,
        to: rule.to,
        enabled: rule.enabled ?? true,
        wholeWord: rule.wholeWord ?? true,
      });
      resetReplacementForm();
      await refresh();
    } catch (error) {
      setMessage(errorMessage(error), true);
    } finally {
      state.busy = false;
      render();
    }
  }

  async function deleteReplacement(id) {
    if (!invokeCommand || state.busy) return;
    state.busy = true;
    try {
      await invokeCommand("replacement_delete", { id });
      resetReplacementForm();
      await refresh();
    } catch (error) {
      setMessage(errorMessage(error), true);
    } finally {
      state.busy = false;
      render();
    }
  }

  elements.dictionaryForm.addEventListener("submit", (event) => {
    event.preventDefault();
    const term = elements.dictionaryTerm.value.trim();
    if (!term) {
      setMessage("Dictionary term cannot be blank.", true);
      return;
    }
    void saveDictionary({
      id: elements.dictionaryId.value ? Number(elements.dictionaryId.value) : null,
      term,
      aliases: aliasesFromInput(elements.dictionaryAliases.value),
      enabled: elements.dictionaryEnabled.checked,
    });
  });

  elements.replacementForm.addEventListener("submit", (event) => {
    event.preventDefault();
    const from = elements.replacementFrom.value.trim();
    const to = elements.replacementTo.value.trim();
    if (!from || !to) {
      setMessage("Replacement source and destination are required.", true);
      return;
    }
    void saveReplacement({
      id: elements.replacementId.value ? Number(elements.replacementId.value) : null,
      from,
      to,
      enabled: elements.replacementEnabled.checked,
      wholeWord: elements.replacementWholeWord.checked,
    });
  });

  elements.dictionaryCancel.addEventListener("click", resetDictionaryForm);
  elements.replacementCancel.addEventListener("click", resetReplacementForm);
  elements.search.addEventListener("input", render);
  elements.refresh.addEventListener("click", () => void refresh());

  resetDictionaryForm();
  resetReplacementForm();
  void refresh();
})();
