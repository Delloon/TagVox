/**
 * Вьюха «Звуки»: загрузка аудиофайлов в конечную папку (sounds/) и список
 * уже загруженных файлов с предпрослушиванием и удалением.
 */


const ALLOWED_SOUND_EXT = ["mp3", "ogg", "wav", "flac"];

Views.sounds = {
  title: "Звуки",
  subtitle: "Локальные аудиофайлы для голосовых команд",

  async mount(root) {
    root.innerHTML = `
      <div class="panel">
        <div class="panel-header">
          <div>
            <h2>Загрузка</h2>
            <div class="desc">Поддерживаются форматы: ${ALLOWED_SOUND_EXT.join(", ")}</div>
          </div>
        </div>
        <div class="dropzone" id="dropzone">
          <div class="title">Перетащи файл сюда или нажми, чтобы выбрать</div>
          <div class="sub">Файл появится в списке ниже и будет доступен для голосовых команд</div>
          <input type="file" id="file-input" accept=".mp3,.ogg,.wav,.flac" multiple />
        </div>
        <div id="upload-progress-container"></div>
      </div>

      <div class="panel">
        <div class="panel-header">
          <div>
            <h2>Загруженные файлы</h2>
          </div>
          <button class="btn btn-sm" type="button" id="refresh-sounds">Обновить</button>
        </div>
        <div id="sound-list-container"></div>
      </div>
    `;

    this.setupDropzone(root);
    await this.loadSoundList(root);

    root.querySelector("#refresh-sounds").addEventListener("click", () => this.loadSoundList(root));
  },

  async loadSoundList(root) {
    const container = root.querySelector("#sound-list-container");
    await withLoading(container, 3, async () => {
      let sounds;
      try {
        sounds = await Api.getSounds();
      } catch (err) {
        renderErrorState(container, err.message, () => this.loadSoundList(root));
        return;
      }

      if (sounds.length === 0) {
        renderEmptyState(container, "Пока нет ни одного файла", "Загрузи первый звук через форму выше.");
        return;
      }

      const list = elFromHtml('<div class="sound-list"></div>');
      sounds.forEach((sound) => {
        const row = elFromHtml(`
          <div class="sound-row">
            <span class="name">${escapeHtml(sound.name)}</span>
            <audio controls preload="none" src="/sounds/${encodeURIComponent(sound.name)}"></audio>
            <span class="size">${formatBytes(sound.size_bytes)}</span>
            <button class="btn btn-sm btn-danger" type="button">Удалить</button>
          </div>
        `);
        row.querySelector("button").addEventListener("click", async () => {
          if (!confirm(`Удалить файл «${sound.name}»? Голосовые команды, ссылающиеся на него, перестанут работать.`)) {
            return;
          }
          try {
            await Api.deleteSound(sound.name);
            row.remove();
            App.toast("Файл удалён", "success");
            if (list.children.length === 0) {
              renderEmptyState(container, "Пока нет ни одного файла", "Загрузи первый звук через форму выше.");
            }
          } catch (err) {
            App.toast(err.message, "error");
          }
        });
        list.appendChild(row);
      });

      container.innerHTML = "";
      container.appendChild(list);
    });
  },

  setupDropzone(root) {
    const dropzone = root.querySelector("#dropzone");
    const fileInput = root.querySelector("#file-input");

    dropzone.addEventListener("click", () => fileInput.click());

    dropzone.addEventListener("dragover", (e) => {
      e.preventDefault();
      dropzone.classList.add("dragover");
    });
    dropzone.addEventListener("dragleave", () => dropzone.classList.remove("dragover"));
    dropzone.addEventListener("drop", (e) => {
      e.preventDefault();
      dropzone.classList.remove("dragover");
      this.handleFiles(root, e.dataTransfer.files);
    });

    fileInput.addEventListener("change", () => {
      this.handleFiles(root, fileInput.files);
      fileInput.value = "";
    });
  },

  handleFiles(root, fileList) {
    const files = Array.from(fileList);
    for (const file of files) {
      const ext = file.name.split(".").pop().toLowerCase();
      if (!ALLOWED_SOUND_EXT.includes(ext)) {
        App.toast(`«${file.name}» — неподдерживаемый формат`, "error");
        continue;
      }
      this.uploadFile(root, file);
    }
  },

  uploadFile(root, file) {
    const progressContainer = root.querySelector("#upload-progress-container");
    const progressEl = elFromHtml(`
      <div class="upload-progress">
        <div class="filename">
          <span>${escapeHtml(file.name)}</span>
          <span class="percent">0%</span>
        </div>
        <div class="bar-track"><div class="bar-fill"></div></div>
      </div>
    `);
    progressContainer.appendChild(progressEl);

    const fill = progressEl.querySelector(".bar-fill");
    const percentLabel = progressEl.querySelector(".percent");

    Api.uploadSound(file, (percent) => {
      fill.style.width = percent + "%";
      percentLabel.textContent = percent + "%";
    })
      .then(() => {
        App.toast(`«${file.name}» загружен`, "success");
        progressEl.remove();
        this.loadSoundList(root);
      })
      .catch((err) => {
        percentLabel.textContent = "ошибка";
        percentLabel.style.color = "var(--danger)";
        App.toast(`${file.name}: ${err.message}`, "error");
      });
  },
};