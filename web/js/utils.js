/**
 * Общие вспомогательные функции, используемые во всех вьюхах.
 */

// Единственное место, где объявляется идентификатор `Views` — остальные
// файлы (views/*.js) только присваивают в него свойства (Views.dashboard = ...),
// а не объявляют заново: повторное `const`/`let` с тем же именем в другом
// <script>-теге — это SyntaxError, весь файл после этого не выполнится.
const Views = window.Views || (window.Views = {});

/** Создаёт DOM-элемент из HTML-строки (первый корневой элемент). */
function elFromHtml(html) {
  const template = document.createElement("template");
  template.innerHTML = html.trim();
  return template.content.firstElementChild;
}

/** Экранирует спецсимволы HTML — обязательно для любого текста от API. */
function escapeHtml(value) {
  const div = document.createElement("div");
  div.textContent = value ?? "";
  return div.innerHTML;
}

/** Форматирует байты в человекочитаемый вид: 1536 -> "1.5 KB". */
function formatBytes(bytes) {
  if (bytes === 0) return "0 B";
  if (bytes === null || bytes === undefined) return "—";
  const units = ["B", "KB", "MB", "GB"];
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / Math.pow(1024, exp);
  return `${exp === 0 ? value : value.toFixed(1)} ${units[exp]}`;
}

/** Форматирует секунды в "1ч 24м" / "42м" / "12с". */
function formatDuration(totalSeconds) {
  if (totalSeconds === null || totalSeconds === undefined) return "—";
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor((totalSeconds % 3600) / 60);
  const s = Math.floor(totalSeconds % 60);
  if (h > 0) return `${h}ч ${m}м`;
  if (m > 0) return `${m}м ${s}с`;
  return `${s}с`;
}

/** Простой debounce для полей ввода. */
function debounce(fn, wait) {
  let timer = null;
  return (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), wait);
  };
}

/** Генерирует короткий читаемый id из строки-триггера (для новых записей). */
function slugify(text) {
  const translit = {
    а: "a", б: "b", в: "v", г: "g", д: "d", е: "e", ё: "e", ж: "zh", з: "z",
    и: "i", й: "y", к: "k", л: "l", м: "m", н: "n", о: "o", п: "p", р: "r",
    с: "s", т: "t", у: "u", ф: "f", х: "h", ц: "c", ч: "ch", ш: "sh", щ: "sch",
    ъ: "", ы: "y", ь: "", э: "e", ю: "yu", я: "ya",
  };
  const transliterated = text
    .toLowerCase()
    .split("")
    .map((ch) => translit[ch] ?? ch)
    .join("");
  const slug = transliterated
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 32);
  return slug || `item-${Date.now().toString(36)}`;
}

/** Показывает панель загрузки (скелетон) в контейнере на время выполнения promise. */
async function withLoading(container, lines, promiseFn) {
  container.innerHTML = Array.from({ length: lines })
    .map(() => `<div class="skeleton" style="margin-bottom:10px;"></div>`)
    .join("");
  return promiseFn();
}

/** Рендерит блок ошибки с кнопкой повтора. */
function renderErrorState(container, message, onRetry) {
  const block = elFromHtml(`
    <div class="state-block state-error">
      <div class="title">Не получилось загрузить данные</div>
      <div>${escapeHtml(message)}</div>
      <div class="action"><button class="btn btn-sm" type="button">Повторить</button></div>
    </div>
  `);
  block.querySelector("button").addEventListener("click", onRetry);
  container.innerHTML = "";
  container.appendChild(block);
}

/** Рендерит пустое состояние. */
function renderEmptyState(container, title, description) {
  container.innerHTML = `
    <div class="state-block">
      <div class="title">${escapeHtml(title)}</div>
      <div>${escapeHtml(description)}</div>
    </div>
  `;
}

/**
 * Создаёт интерактивный chip-input: показывает существующие элементы как
 * чипы с крестиком удаления, плюс текстовое поле — Enter или запятая
 * добавляют новый чип. Возвращает { getItems() } для чтения текущего списка.
 */
function createChipInput(container, initialItems, placeholder) {
  let items = [...initialItems];

  function renderChips() {
    container.querySelectorAll(".chip").forEach((chip) => chip.remove());
    const input = container.querySelector("input");
    items.forEach((item, index) => {
      const chip = elFromHtml(`
        <span class="chip">
          <span>${escapeHtml(item)}</span>
          <button type="button" aria-label="Удалить">×</button>
        </span>
      `);
      chip.querySelector("button").addEventListener("click", () => {
        items.splice(index, 1);
        renderChips();
      });
      container.insertBefore(chip, input);
    });
  }

  container.innerHTML = `<input type="text" placeholder="${escapeHtml(placeholder)}" />`;
  const input = container.querySelector("input");
  renderChips();

  function tryAdd() {
    const value = input.value.trim().replace(/,$/, "");
    if (value) {
      items.push(value);
      input.value = "";
      renderChips();
    }
  }

  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      tryAdd();
    } else if (e.key === "Backspace" && input.value === "" && items.length > 0) {
      items.pop();
      renderChips();
    }
  });

  input.addEventListener("blur", tryAdd);

  return {
    getItems: () => items.filter(Boolean),
  };
}