/**
 * Тонкая обёртка над fetch, реализующая контракт из API.md.
 * Ничего не знает про DOM — только сеть.
 */

class ApiError extends Error {
  constructor(message, status) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

const Api = (() => {
  const BASE = "/api";

  async function request(method, path, body, isMultipart = false) {
    const options = {
      method,
      credentials: "include",
      headers: {},
    };

    if (body !== undefined) {
      if (isMultipart) {
        options.body = body; // FormData — заголовок Content-Type ставит сам браузер
      } else {
        options.headers["Content-Type"] = "application/json";
        options.body = JSON.stringify(body);
      }
    }

    let response;
    try {
      response = await fetch(BASE + path, options);
    } catch (networkError) {
      throw new ApiError("Не удалось соединиться с сервером бота.", 0);
    }

    if (response.status === 401) {
      document.dispatchEvent(new CustomEvent("tagvox:unauthorized"));
      throw new ApiError("Сессия истекла, нужно войти заново.", 401);
    }

    if (response.status === 204) {
      return null;
    }

    let data = null;
    const text = await response.text();
    if (text) {
      try {
        data = JSON.parse(text);
      } catch {
        // ответ не JSON — оставляем data как null, ниже используем текст статуса
      }
    }

    if (!response.ok) {
      const message = (data && data.error) || `Сервер ответил ошибкой ${response.status}.`;
      throw new ApiError(message, response.status);
    }

    return data;
  }

  return {
    // --- Аутентификация ---
    getAuthMode: () => request("GET", "/auth/mode"),
    passwordLogin: (password) => request("POST", "/auth/password/login", { password }),
    // Вход через Discord не делается через fetch: это должен быть настоящий
    // переход браузера на /api/auth/discord/login (см. кнопку "Войти через
    // Discord" в index.html) — иначе Discord не сможет показать свою страницу согласия.
    logout: () => request("POST", "/auth/logout"),

    // --- Статус ---
    getStatus: () => request("GET", "/status"),

    // --- Настройки ---
    getConfig: () => request("GET", "/config"),
    saveConfig: (config) => request("PUT", "/config", config),

    // --- Текстовые реакции ---
    getReactions: () => request("GET", "/phrases/reactions"),
    createReaction: (reaction) => request("POST", "/phrases/reactions", reaction),
    updateReaction: (id, reaction) =>
      request("PUT", `/phrases/reactions/${encodeURIComponent(id)}`, reaction),
    deleteReaction: (id) => request("DELETE", `/phrases/reactions/${encodeURIComponent(id)}`),

    // --- Голосовые команды ---
    getVoiceCommands: () => request("GET", "/phrases/voice-commands"),
    createVoiceCommand: (cmd) => request("POST", "/phrases/voice-commands", cmd),
    updateVoiceCommand: (id, cmd) =>
      request("PUT", `/phrases/voice-commands/${encodeURIComponent(id)}`, cmd),
    deleteVoiceCommand: (id) =>
      request("DELETE", `/phrases/voice-commands/${encodeURIComponent(id)}`),

    // --- Фраза-заглушка ---
    getUnknownResponses: () => request("GET", "/phrases/unknown-responses"),
    saveUnknownResponses: (list) => request("PUT", "/phrases/unknown-responses", list),

    // --- Звуки ---
    getSounds: () => request("GET", "/sounds"),
    uploadSound: (file, onProgress) =>
      new Promise((resolve, reject) => {
        const xhr = new XMLHttpRequest();
        xhr.open("POST", BASE + "/sounds");
        xhr.withCredentials = true;
        xhr.upload.addEventListener("progress", (e) => {
          if (e.lengthComputable && onProgress) {
            onProgress(Math.round((e.loaded / e.total) * 100));
          }
        });
        xhr.addEventListener("load", () => {
          if (xhr.status === 401) {
            document.dispatchEvent(new CustomEvent("tagvox:unauthorized"));
            reject(new ApiError("Сессия истекла, нужно войти заново.", 401));
            return;
          }
          if (xhr.status >= 200 && xhr.status < 300) {
            try {
              resolve(JSON.parse(xhr.responseText));
            } catch {
              resolve(null);
            }
          } else {
            let message = `Сервер ответил ошибкой ${xhr.status}.`;
            try {
              const parsed = JSON.parse(xhr.responseText);
              if (parsed && parsed.error) message = parsed.error;
            } catch {
              /* ignore */
            }
            reject(new ApiError(message, xhr.status));
          }
        });
        xhr.addEventListener("error", () => {
          reject(new ApiError("Не удалось соединиться с сервером бота.", 0));
        });
        const formData = new FormData();
        formData.append("file", file);
        xhr.send(formData);
      }),
    deleteSound: (name) => request("DELETE", `/sounds/${encodeURIComponent(name)}`),

    // --- Серверы / анонимные сообщения ---
    getGuilds: () => request("GET", "/guilds"),
    getGuildChannels: (guildId) => request("GET", `/guilds/${encodeURIComponent(guildId)}/channels`),
    getGuildRoles: (guildId) => request("GET", `/guilds/${encodeURIComponent(guildId)}/roles`),
    getAnonSettings: (guildId) => request("GET", `/anon/settings/${encodeURIComponent(guildId)}`),
    saveAnonSettings: (guildId, settings) => request("PUT", `/anon/settings/${encodeURIComponent(guildId)}`, settings),
  };
})();