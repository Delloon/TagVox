/**
 * Главный контроллер: экран логина, переключение вкладок, тосты,
 * первичная проверка сессии.
 */

const App = (() => {
  const viewMap = {
    dashboard: Views.dashboard,
    settings: Views.settings,
    phrases: Views.phrases,
    "voice-commands": Views.voiceCommands,
    sounds: Views.sounds,
    anon: Views.anon,
    modules: Views.modules,
  };

  let currentViewKey = "dashboard";
  let authMode = "discord";

  const loginScreen = document.getElementById("login-screen");
  const appRoot = document.getElementById("app");
  const viewRoot = document.getElementById("view-root");
  const viewTitle = document.getElementById("view-title");
  const viewSubtitle = document.getElementById("view-subtitle");
  const toastContainer = document.getElementById("toast-container");
  const sidebarStatus = document.getElementById("sidebar-status");
  const sidebarStatusText = document.getElementById("sidebar-status-text");

  function toast(message, type = "default") {
    const el = elFromHtml(`<div class="toast ${type === "error" ? "toast-error" : type === "success" ? "toast-success" : ""}"></div>`);
    el.textContent = message;
    toastContainer.appendChild(el);
    setTimeout(() => {
      el.style.transition = "opacity 0.2s ease";
      el.style.opacity = "0";
      setTimeout(() => el.remove(), 200);
    }, 3200);
  }

  function applyLoginMode(mode) {
    authMode = mode;
    const discordBtn = document.getElementById("login-discord-btn");
    const passwordForm = document.getElementById("login-password-form");
    const hint = document.getElementById("login-hint");

    if (mode === "password") {
      discordBtn.hidden = true;
      passwordForm.hidden = false;
      hint.textContent = "Введи пароль администратора панели.";
    } else {
      discordBtn.hidden = false;
      passwordForm.hidden = true;
      hint.textContent = "Вход через Discord — доступ только у аккаунтов из списка администраторов бота.";
    }
  }

  function showLogin(message) {
    appRoot.hidden = true;
    loginScreen.hidden = false;
    const errorField = document.getElementById("login-error-field");
    const errorText = document.getElementById("login-error");
    if (message) {
      errorField.style.display = "block";
      errorText.textContent = message;
    } else {
      errorField.style.display = "none";
      errorText.textContent = "";
    }
  }

  function showApp() {
    loginScreen.hidden = true;
    appRoot.hidden = false;
  }

  async function navigate(key) {
    const view = viewMap[key];
    if (!view) return;
    currentViewKey = key;

    document.querySelectorAll(".nav-item").forEach((btn) => {
      btn.classList.toggle("active", btn.dataset.view === key);
    });

    viewTitle.textContent = view.title;
    viewSubtitle.textContent = view.subtitle || "";
    viewRoot.innerHTML = "";
    await view.mount(viewRoot);
  }

  async function refreshSidebarStatus() {
    try {
      const status = await Api.getStatus();
      sidebarStatus.classList.toggle("online", !!status.online);
      sidebarStatus.classList.toggle("offline", !status.online);
      sidebarStatusText.textContent = status.online
        ? `онлайн · ${status.ping_ms ?? "—"} мс`
        : "офлайн";
    } catch {
      sidebarStatus.classList.remove("online");
      sidebarStatus.classList.add("offline");
      sidebarStatusText.textContent = "нет соединения";
    }
  }

  function setupNav() {
    document.querySelectorAll(".nav-item").forEach((btn) => {
      btn.addEventListener("click", () => navigate(btn.dataset.view));
    });
  }

  function setupPasswordLogin() {
    const form = document.getElementById("login-password-form");
    form.addEventListener("submit", async (e) => {
      e.preventDefault();
      const password = document.getElementById("login-password").value;
      const submitBtn = document.getElementById("login-password-submit");
      submitBtn.disabled = true;
      try {
        await Api.passwordLogin(password);
        showApp();
        await navigate(currentViewKey);
        await refreshSidebarStatus();
      } catch (err) {
        showLogin(err.status === 401 ? "Неверный пароль" : err.message);
      } finally {
        submitBtn.disabled = false;
      }
    });
  }

  function setupLogout() {
    document.getElementById("logout-btn").addEventListener("click", async () => {
      try {
        await Api.logout();
      } catch {
        /* даже если запрос не удался, всё равно показываем логин локально */
      }
      showLogin();
    });
  }

  function setupUnauthorizedHandler() {
    document.addEventListener("tagvox:unauthorized", () => {
      showLogin("Сессия истекла, войдите заново");
    });
  }

  async function init() {
    setupNav();
    setupPasswordLogin();
    setupLogout();
    setupUnauthorizedHandler();

    // Узнаём, какой экран логина показывать (Discord-кнопка или пароль)
    try {
      const { mode } = await Api.getAuthMode();
      applyLoginMode(mode);
    } catch {
      applyLoginMode("discord");
    }

    // Discord мог вернуть нас на "/" с ?login_error=... (пользователь
    // отклонил доступ или Discord прислал ошибку) — покажем её на экране входа.
    const params = new URLSearchParams(window.location.search);
    const loginErrorParam = params.get("login_error");
    if (loginErrorParam) {
      window.history.replaceState({}, "", window.location.pathname);
    }

    // Проверяем, есть ли уже активная сессия (кука от предыдущего входа)
    try {
      await Api.getStatus();
      showApp();
      await navigate("dashboard");
      await refreshSidebarStatus();
      setInterval(refreshSidebarStatus, 30000);
    } catch (err) {
      if (err.status === 401) {
        showLogin(loginErrorParam ? `Discord отклонил вход: ${loginErrorParam}` : null);
      } else {
        // Сервер недоступен вовсе (бэкенд ещё не запущен) — всё равно
        // показываем экран логина, а не зависаем на пустом экране.
        showLogin("Не удалось связаться с сервером бота. Проверь, что бэкенд запущен.");
      }
    }
  }

  return { navigate, toast, init };
})();

document.addEventListener("DOMContentLoaded", () => App.init());