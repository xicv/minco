// Minco Ticketing agent console. Dependency-free, same-origin, no credentials
// in script: the host authenticates requests. Every control calls a real
// operation; there is no polling.
(function () {
  "use strict";

  var BASE = "/_minco/ticketing";
  var VIEWS = {
    active: ["new", "open", "pending_internal"],
    mine: [],
    waiting: ["pending_requester", "on_hold"],
    resolved: ["resolved", "closed"],
    all: []
  };
  var PAGE_SIZE = 25;

  var state = {
    view: "active",
    cursor: null,
    page: [],
    selected: null,
    etag: null,
    principalSubject: null,
    capabilities: { create: false, reply: false, internal_note: false, manage: false }
  };

  function el(name) { return document.querySelector("[data-console=\"" + name + "\"]"); }

  function setStatus(message) { el("status").textContent = message || ""; }

  // Every request owns its AbortController: unrelated list, detail and
  // mutation operations never cancel each other (review finding 12).
  function fetchJson(path, options) {
    var controller = new AbortController();
    options = options || {};
    options.signal = controller.signal;
    options.credentials = "same-origin";
    return fetch(path, options).then(function (response) {
      var etag = response.headers.get("ETag");
      if (!response.ok) {
        return response.json().then(function (problem) {
          var error = new Error((problem && (problem.title || problem.code)) || "Request failed");
          error.status = response.status;
          throw error;
        }, function () { throw new Error("Request failed"); });
      }
      return response.json().then(function (body) {
        return { body: body, etag: etag };
      });
    });
  }

  function listQuery() {
    var params = new URLSearchParams();
    params.set("page[limit]", String(PAGE_SIZE));
    VIEWS[state.view].forEach(function (status) { params.append("filter[status]", status); });
    if (state.view === "mine" && state.principalSubject) {
      params.set("filter[assignee_subject]", state.principalSubject);
    }
    if (state.cursor) { params.set("page[after]", state.cursor); }
    return params.toString();
  }

  function formatTime(value) {
    return new Date(value).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  }

  function renderList(summaries) {
    var query = el("search").value.trim().toLowerCase();
    var rows = summaries.filter(function (summary) {
      return !query ||
        summary.subject.toLowerCase().indexOf(query) !== -1 ||
        summary.display_reference.toLowerCase().indexOf(query) !== -1 ||
        summary.requester_subject.toLowerCase().indexOf(query) !== -1;
    });
    var body = el("list");
    body.replaceChildren();
    rows.forEach(function (summary) {
      var row = document.createElement("tr");
      row.tabIndex = 0;
      row.setAttribute("role", "button");
      row.dataset.ticketId = summary.id;
      function select() { selectTicket(summary.id); }
      row.addEventListener("click", select);
      row.addEventListener("keydown", function (event) {
        if (event.key === "Enter" || event.key === " ") { event.preventDefault(); select(); }
      });
      [summary.display_reference, summary.subject +
        (summary.needs_attention ? " •" : ""), summary.status, summary.priority,
        summary.assignee_subject || "—", formatTime(summary.updated_at)]
        .forEach(function (value, index) {
          var cell = document.createElement(index === 1 ? "th" : "td");
          if (index === 1) { cell.scope = "row"; }
          cell.textContent = value;
          row.appendChild(cell);
        });
      body.appendChild(row);
    });
    el("empty").hidden = rows.length !== 0;
  }

  function loadPage(reset) {
    if (reset) { state.cursor = null; }
    setStatus("Loading tickets…");
    return fetchJson(BASE + "/agent/tickets?" + listQuery()).then(function (result) {
      state.page = result.body.data;
      renderList(state.page);
      // ResourceCollection page info serializes as camelCase.
      el("next").hidden = !result.body.page.hasMore;
      state.cursor = result.body.page.hasMore ? result.body.page.nextCursor : null;
      setStatus("");
    }, function (error) {
      setStatus(error.status === 403 ? "Console access is forbidden." :
        "Tickets are unavailable. " + error.message);
    });
  }

  function renderDetail(ticket) {
    state.selected = ticket;
    el("detail-panel").hidden = false;
    el("detail-title").textContent = ticket.display_reference + " — " + ticket.subject;
    var meta = el("detail-meta");
    meta.replaceChildren();
    [["Requester", ticket.requester.display_name || ticket.requester.subject],
     ["Status", ticket.status], ["Priority", ticket.priority],
     ["Queue", ticket.queue_id || "—"], ["Assignee", ticket.assignee_subject || "—"],
     ["Updated", formatTime(ticket.updated_at)], ["Revision", String(ticket.revision)]]
      .forEach(function (pair) {
        var term = document.createElement("dt"); term.textContent = pair[0];
        var value = document.createElement("dd"); value.textContent = pair[1];
        meta.appendChild(term); meta.appendChild(value);
      });
    var list = el("detail-messages");
    list.replaceChildren();
    ticket.messages.forEach(function (message) {
      var item = document.createElement("li");
      item.className = message.kind;
      item.textContent = (message.author_subject || "system") + " · " +
        formatTime(message.created_at) + " — " + message.body;
      list.appendChild(item);
    });
  }

  function selectTicket(id) {
    setStatus("Loading ticket…");
    return fetchJson(BASE + "/agent/tickets/" + encodeURIComponent(id)).then(function (result) {
      state.etag = result.etag;
      renderDetail(result.body);
      setStatus("");
    }, function (error) { setStatus("Ticket is unavailable. " + error.message); });
  }

  function sendMutation(path, method, payload, onDone) {
    return fetchJson(path, {
      method: method,
      headers: { "Content-Type": "application/json", "If-Match": state.etag || "" },
      body: JSON.stringify(payload)
    }).then(function (result) {
      state.etag = result.etag;
      onDone(result.body.ticket || result.body);
      setStatus("");
      loadPage(true);
    }, function (error) {
      if (error.status === 412) {
        var reload = state.selected
          ? selectTicket(state.selected.id)
          : Promise.resolve();
        reload.then(function () {
          setStatus("The ticket changed while you were working. Showing the latest version.");
        });
      } else {
        setStatus("The operation was rejected. " + error.message);
      }
    });
  }

  function managementPayload(form) {
    var payload = {};
    var status = form.status.value; if (status) { payload.status = status; }
    var priority = form.priority.value; if (priority) { payload.priority = priority; }
    var assignee = form.assignee_subject.value.trim();
    if (assignee) { payload.assignee_subject = assignee; }
    var queue = form.queue_id.value.trim(); if (queue) { payload.queue_id = queue; }
    var resolution = form.resolution.value.trim(); if (resolution) { payload.resolution = resolution; }
    var closeReason = form.close_reason.value.trim(); if (closeReason) { payload.close_reason = closeReason; }
    return payload;
  }

  // Accessible creation flow (review finding 12): a labelled dialog with
  // required fields, inline validation errors, cancel/Escape support and
  // focus restored to the opener — never window.prompt.
  function openCreateDialog() {
    var dialog = el("create-dialog");
    if (typeof dialog.showModal !== "function") {
      dialog.setAttribute("open", "");
    } else {
      dialog.showModal();
    }
    el("create-error").hidden = true;
    el("create-form").reset();
    el("create-subject").focus();
  }

  function closeCreateDialog() {
    var dialog = el("create-dialog");
    dialog.close();
    // Focus restoration: the opener keeps the keyboard position.
    el("create").focus();
  }

  function submitCreate(event) {
    event.preventDefault();
    var form = el("create-form");
    var error = el("create-error");
    if (!form.subject.value.trim() || form.description.value.trim().length < 20) {
      error.textContent = "A subject and a description of at least 20 characters are required.";
      error.hidden = false;
      return;
    }
    var submit = el("create-submit");
    submit.disabled = true;
    fetchJson(BASE + "/tickets", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        project_id: state.projectId,
        subject: form.subject.value.trim(),
        description: form.description.value.trim(),
        requester: { subject: state.principalSubject || "agent" },
        channel: "internal"
      })
    }).then(function (result) {
      submit.disabled = false;
      closeCreateDialog();
      renderDetail(result.body.ticket);
      state.etag = result.etag;
      setStatus("");
      loadPage(true);
    }, function (requestError) {
      submit.disabled = false;
      error.textContent = "Create was rejected. " + requestError.message;
      error.hidden = false;
    });
  }

  document.addEventListener("DOMContentLoaded", function () {
    el("views").addEventListener("click", function (event) {
      var button = event.target.closest("button[data-view]");
      if (!button) { return; }
      state.view = button.dataset.view;
      el("views").querySelectorAll("button").forEach(function (other) {
        other.setAttribute("aria-pressed", String(other === button));
      });
      loadPage(true);
    });
    el("search").addEventListener("input", function () { renderList(state.page); });
    el("refresh").addEventListener("click", function () { loadPage(true); });
    el("next").addEventListener("click", function () { loadPage(false); });
    el("create").addEventListener("click", openCreateDialog);
    el("create-cancel").addEventListener("click", closeCreateDialog);
    el("create-form").addEventListener("submit", submitCreate);
    el("close-detail").addEventListener("click", function () {
      el("detail-panel").hidden = true;
      state.selected = null;
      // Focus restoration: return to the list controls after closing.
      el("refresh").focus();
    });
    el("reply-form").addEventListener("submit", function (event) {
      event.preventDefault();
      sendMutation(BASE + "/tickets/" + state.selected.id + "/agent-replies", "POST",
        { body: event.target.body.value }, renderDetail);
    });
    el("note-form").addEventListener("submit", function (event) {
      event.preventDefault();
      sendMutation(BASE + "/tickets/" + state.selected.id + "/internal-notes", "POST",
        { body: event.target.body.value }, renderDetail);
    });
    el("manage-form").addEventListener("submit", function (event) {
      event.preventDefault();
      sendMutation(BASE + "/agent/tickets/" + state.selected.id + "/management", "PATCH",
        managementPayload(event.target), renderDetail);
    });

    fetchJson(BASE + "/agent/bootstrap").then(function (result) {
      state.projectId = result.body.project_id;
      state.principalSubject = result.body.subject;
      state.capabilities = result.body.capabilities;
      el("brand").textContent = result.body.brand + " — " + result.body.label;
      // Controls the principal cannot use are hidden, not merely
      // disabled (review finding 12).
      el("create").hidden = !result.body.capabilities.create;
      el("reply-form").hidden = !result.body.capabilities.reply;
      el("note-form").hidden = !result.body.capabilities.internal_note;
      el("manage-form").hidden = !result.body.capabilities.manage;
      loadPage(true);
    }, function (error) {
      setStatus(error.status === 401
        ? "Sign-in required."
        : "The agent console is unavailable. " + error.message);
    });
  });
}());
