"use strict";

const layoutStylesheet = document.createElement("link");
layoutStylesheet.rel = "stylesheet";
layoutStylesheet.href = "layout.css";
document.head.append(layoutStylesheet);

const navItems = Array.from(document.querySelectorAll("[data-view-target]"));
const views = Array.from(document.querySelectorAll("[data-view]"));
const VIEW_STORAGE_KEY = "blcvoice.desktop.view";

function activateView(name, { remember = true } = {}) {
  const target = views.find((view) => view.dataset.view === name) ?? views[0];
  if (!target) return;

  for (const view of views) {
    view.hidden = view !== target;
  }

  for (const item of navItems) {
    const active = item.dataset.viewTarget === target.dataset.view;
    item.classList.toggle("active", active);
    if (active) item.setAttribute("aria-current", "page");
    else item.removeAttribute("aria-current");
  }

  document.querySelector(".workspace")?.scrollTo({ top: 0, behavior: "instant" });
  if (remember) {
    try {
      window.localStorage.setItem(VIEW_STORAGE_KEY, target.dataset.view || "home");
    } catch {
      // Navigation remains functional when storage is unavailable.
    }
  }
}

for (const item of navItems) {
  item.addEventListener("click", () => activateView(item.dataset.viewTarget));
}

let initialView = "home";
try {
  initialView = window.localStorage.getItem(VIEW_STORAGE_KEY) || "home";
} catch {
  initialView = "home";
}

activateView(initialView, { remember: false });
