"use strict";

const laneOrder = [
  ["source", "Source"],
  ["local_verification", "Local"],
  ["hosted_verification", "Hosted"],
  ["deployment", "Deployment"],
  ["runtime", "Runtime"],
  ["review", "Review"],
];

const state = { view: null, speaking: false, activeView: "overview" };

function element(id) {
  return document.getElementById(id);
}

function setText(id, value) {
  element(id).textContent = String(value);
}

function make(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = String(text);
  return node;
}

function taskCounts(view) {
  const counts = view.summary.task_status_counts || {};
  return {
    complete: Number(counts.complete || 0),
    planned: Number(counts.planned || 0),
    total: Number(view.summary.denominator || 0),
  };
}

function renderSummary(view) {
  const counts = taskCounts(view);
  setText("project-name", view.project.name);
  setText("source-digest", `${view.project.source_digest.slice(0, 8)}…`);
  setText("node-count", view.summary.node_count);
  setText("edge-count", view.summary.edge_count);
  setText("task-count", counts.total);
  setText("complete-count", counts.complete);
  setText("planned-count", counts.planned);
  setText("progress-complete", counts.complete);
  setText("progress-total", counts.total);
  const percent = counts.total === 0 ? 0 : Math.round((counts.complete / counts.total) * 100);
  element("progress-fill").style.width = `${percent}%`;
  element("progress-track").setAttribute("aria-valuenow", String(percent));
  setText("status-schema", `Schema ${view.schema_version}`);
  setText("status-files", `${view.input_usage.files.toLocaleString()} files`);
  setText("status-bytes", `${view.input_usage.bytes.toLocaleString()} bytes`);

  const rawStatuses = element("raw-statuses");
  rawStatuses.replaceChildren();
  Object.entries(view.summary.task_status_counts || {}).forEach(([status, count]) => {
    const row = make("div");
    const label = make("dt", status === "planned" ? "planned" : "", status);
    const value = make("dd", status === "planned" ? "planned" : "", count);
    row.append(label, value);
    rawStatuses.append(row);
  });
}

function architectureCount(view, layer) {
  return view.nodes.filter((node) => node.kind === "architecture" && node.properties && node.properties.layer === layer).length;
}

function renderTopology(view) {
  const topology = element("topology");
  topology.replaceChildren();
  const groups = [
    ["OpenAPI", view.nodes.filter((node) => node.kind === "operation").length, "operation nodes"],
    ["Application", architectureCount(view, "application"), "declared roots"],
    ["Domain", architectureCount(view, "domain"), "declared roots"],
    ["Adapters", view.nodes.filter((node) => node.kind === "resource").length, "resource nodes"],
    ["Plan IR", view.deployment && view.deployment.plan ? 1 : 0, "deployment projection"],
    ["Plugins", view.nodes.filter((node) => node.kind === "feature").length, "feature nodes"],
  ];
  groups.forEach(([label, count, description], index) => {
    const node = make("button", "topology-node", undefined);
    node.type = "button";
    if (index === 1) node.setAttribute("aria-current", "true");
    if (count > 0) node.classList.add("is-complete");
    node.append(make("strong", "", label), make("small", "", `${count} ${description}`));
    node.addEventListener("click", () => {
      topology.querySelectorAll("[aria-current]").forEach((item) => item.removeAttribute("aria-current"));
      node.setAttribute("aria-current", "true");
    });
    topology.append(node);
  });
  setText("graph-caption", `${view.summary.node_count} nodes · ${view.summary.edge_count} edges`);
}

function renderEvidence(view) {
  const grid = element("evidence-grid");
  grid.replaceChildren();
  laneOrder.forEach(([key, label]) => {
    const items = Array.isArray(view.evidence[key]) ? view.evidence[key] : [];
    const lane = make("section", "evidence-lane");
    lane.setAttribute("aria-label", `${label} evidence, ${items.length} items`);
    const heading = make("h3");
    heading.append(make("span", "", label), make("span", "", items.length));
    const list = make("ul");
    const visible = items.slice(0, 4);
    if (visible.length === 0) {
      list.append(make("li", "evidence-empty", "No evidence in this snapshot"));
    } else {
      visible.forEach((item) => {
        const row = make("li", "", item.subject || item.source || "Evidence item");
        row.title = `${item.state || "unknown"} · ${item.source || "unknown source"}`;
        list.append(row);
      });
    }
    if (items.length > visible.length) list.append(make("li", "", `+ ${items.length - visible.length} more`));
    lane.append(heading, list);
    grid.append(lane);
  });
}

function renderDetail(viewName) {
  const view = state.view;
  const title = element("detail-title");
  const description = element("detail-description");
  const content = element("detail-content");
  content.replaceChildren();
  const list = make("ul", "detail-list");
  const definitions = {
    architecture: ["Architecture", "Declared bounded architecture roots.", view.nodes.filter((node) => node.kind === "architecture")],
    operations: ["Operations", "OpenAPI operations from canonical contracts.", view.nodes.filter((node) => node.kind === "operation")],
    tasks: ["Tasks", "Raw task statuses and derived readiness.", view.nodes.filter((node) => node.kind === "task")],
    evidence: ["Evidence", "Six independent evidence lanes.", laneOrder.flatMap(([key]) => view.evidence[key] || [])],
    deployment: ["Deployment", "Read-only deployment and cost projection.", view.deployment.diagnostics || []],
    feedback: ["Feedback", view.feedback.limitation || "Feedback capability metadata.", (view.feedback.operation_ids || []).map((id) => ({ label: id, description: "Declared Feedback operation", raw_status: view.feedback.enabled ? "enabled" : "disabled" }))],
  };
  const [heading, summary, items] = definitions[viewName] || definitions.architecture;
  title.textContent = heading;
  description.textContent = summary;
  items.slice(0, 200).forEach((item) => {
    const row = make("li");
    const identity = item.label || item.subject || item.code || "Item";
    const detail = item.description || item.source || item.message || "Bounded ProjectView item";
    const status = item.raw_status || item.state || item.severity || "—";
    row.append(make("code", "", identity), make("span", "", detail), make("span", "", status));
    list.append(row);
  });
  if (items.length === 0) list.append(make("li", "", "No items in this snapshot."));
  content.append(list);
}

function activateView(name) {
  state.activeView = name;
  document.querySelectorAll(".nav-item").forEach((button) => {
    const active = button.dataset.view === name;
    button.classList.toggle("is-selected", active);
    if (active) button.setAttribute("aria-current", "page");
    else button.removeAttribute("aria-current");
  });
  const overview = name === "overview";
  element("overview-panel").hidden = !overview;
  element("detail-panel").hidden = overview;
  if (!overview && state.view) renderDetail(name);
  element("main-content").focus({ preventScroll: true });
}

function activateMobileSection(name) {
  document.querySelectorAll("[data-mobile-section]").forEach((button) => {
    const active = button.dataset.mobileSection === name;
    button.setAttribute("aria-selected", String(active));
    button.tabIndex = active ? 0 : -1;
  });
  document.querySelectorAll("[data-mobile-panel]").forEach((panel) => {
    panel.classList.toggle("is-mobile-visible", panel.dataset.mobilePanel === name);
  });
}

function setupNavigation() {
  const navButtons = Array.from(document.querySelectorAll(".nav-item"));
  navButtons.forEach((button, index) => {
    button.addEventListener("click", () => activateView(button.dataset.view));
    button.addEventListener("keydown", (event) => {
      if (event.key !== "ArrowDown" && event.key !== "ArrowRight" && event.key !== "ArrowUp" && event.key !== "ArrowLeft") return;
      event.preventDefault();
      const direction = event.key === "ArrowDown" || event.key === "ArrowRight" ? 1 : -1;
      navButtons[(index + direction + navButtons.length) % navButtons.length].focus();
    });
  });
  const mobileTabs = Array.from(document.querySelectorAll("[data-mobile-section]"));
  mobileTabs.forEach((button, index) => {
    button.addEventListener("click", () => activateMobileSection(button.dataset.mobileSection));
    button.addEventListener("keydown", (event) => {
      let targetIndex;
      if (event.key === "ArrowRight") targetIndex = (index + 1) % mobileTabs.length;
      else if (event.key === "ArrowLeft") targetIndex = (index - 1 + mobileTabs.length) % mobileTabs.length;
      else if (event.key === "Home") targetIndex = 0;
      else if (event.key === "End") targetIndex = mobileTabs.length - 1;
      else return;
      event.preventDefault();
      const target = mobileTabs[targetIndex];
      activateMobileSection(target.dataset.mobileSection);
      target.focus();
    });
  });
  activateMobileSection("graph");
}

function setupReadAloud() {
  const button = element("read-aloud-button");
  if (!("speechSynthesis" in window) || !("SpeechSynthesisUtterance" in window)) {
    button.disabled = true;
    button.title = "Read aloud is unavailable in this browser";
    return;
  }
  button.addEventListener("click", () => {
    if (state.speaking) {
      window.speechSynthesis.cancel();
      state.speaking = false;
      button.textContent = "Read aloud";
      button.setAttribute("aria-pressed", "false");
      return;
    }
    const utterance = new SpeechSynthesisUtterance(element("main-content").innerText);
    utterance.addEventListener("end", () => {
      state.speaking = false;
      button.textContent = "Read aloud";
      button.setAttribute("aria-pressed", "false");
    });
    state.speaking = true;
    button.textContent = "Stop reading";
    button.setAttribute("aria-pressed", "true");
    window.speechSynthesis.speak(utterance);
  });
}

function setupExport() {
  element("export-button").addEventListener("click", () => {
    if (!state.view) return;
    const data = new Blob([JSON.stringify(state.view)], { type: "application/json" });
    const url = URL.createObjectURL(data);
    const link = document.createElement("a");
    link.href = url;
    link.download = "project-view.json";
    link.click();
    URL.revokeObjectURL(url);
  });
}

async function loadProjectView() {
  try {
    const response = await fetch("project-view.json", { cache: "no-store", credentials: "same-origin" });
    if (!response.ok) throw new Error(`ProjectView request failed with ${response.status}`);
    const view = await response.json();
    if (view.schema_version !== 1) throw new Error(`Unsupported ProjectView schema ${view.schema_version}`);
    state.view = view;
    renderSummary(view);
    renderTopology(view);
    renderEvidence(view);
  } catch (error) {
    const alert = element("load-error");
    alert.textContent = error instanceof Error ? error.message : "Could not load ProjectView";
    alert.hidden = false;
  }
}

setupNavigation();
setupReadAloud();
setupExport();
loadProjectView();
