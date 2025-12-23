let evtSource;

function initSse() {
  evtSource = new EventSource("/sse");

  evtSource.addEventListener("reload", (_event) => {
    console.warn("Reload event");
    location.reload();
  });

  evtSource.addEventListener("status", (_event) => {
    const elements = document.querySelectorAll("[data-sse=status]");

    for (const element of elements) {
      htmx.trigger(element, "status");
    }
  });

  evtSource.addEventListener("tracklist", (_event) => {
    const elements = document.querySelectorAll("[data-sse=tracklist]");

    for (const element of elements) {
      htmx.trigger(element, "tracklist");
    }
  });

  evtSource.addEventListener("volume", (event) => {
    const slider = document.getElementById("volume-slider");
    if (slider === null) {
      return;
    }
    slider.value = event.data;
  });

  evtSource.addEventListener("error", (event) => {
    htmx.swap("#toast-container", event.data, { swapStyle: "afterbegin" });
  });

  evtSource.addEventListener("warn", (event) => {
    htmx.swap("#toast-container", event.data, { swapStyle: "afterbegin" });
  });

  evtSource.addEventListener("success", (event) => {
    htmx.swap("#toast-container", event.data, { swapStyle: "afterbegin" });
  });

  evtSource.addEventListener("info", (event) => {
    htmx.swap("#toast-container", event.data, { swapStyle: "afterbegin" });
  });

  evtSource.addEventListener("position", (event) => {
    const slider = document.getElementById("progress-slider");
    if (slider === null) {
      return;
    }
    slider.value = event.data;

    const positionElement = document.getElementById("position");

    const seconds = event.data / 1000;

    const minutesString = Math.floor(seconds / 60)
      .toString()
      .padStart(2, "0");
    const secondsString = Math.floor(seconds % 60)
      .toString()
      .padStart(2, "0");

    positionElement.innerText = `${minutesString}:${secondsString}`;
  });
}

initSse();

function refreshSse() {
  const elements = document.querySelectorAll("[hx-trigger='tracklist'");

  for (const element of elements) {
    htmx.trigger(element, "tracklist");
  }

  const statusElements = document.querySelectorAll("[hx-trigger='status'");

  for (const element of statusElements) {
    htmx.trigger(element, "status");
  }
}

document.addEventListener("visibilitychange", () => {
  if (!document.hidden) {
    initSse();
    refreshSse();
  }
});

function focusSearchInput() {
  document.getElementById("query").focus();
}

function loadSearchInput() {
  let value = sessionStorage.getItem("search-query");
  document.getElementById("query").value = value;
}

function setSearchQuery(value) {
  sessionStorage.setItem("search-query", value);

  const url = new URL(window.location.href);
  if (value && value.trim() !== "") {
    url.searchParams.set("query", value);
  } else {
    url.searchParams.delete("query");
  }

  history.replaceState(null, "", url.toString());

  document.getElementById("albums-tab").href = "albums?query=" + value;
  document.getElementById("artists-tab").href = "artists?query=" + value;
  document.getElementById("playlists-tab").href = "playlists?query=" + value;
  document.getElementById("tracks-tab").href = "tracks?query=" + value;
}

htmx.onLoad(function (content) {
  var sortables = content.querySelectorAll(".sortable");
  for (var i = 0; i < sortables.length; i++) {
    var sortable = sortables[i];
    var sortableInstance = new Sortable(sortable, {
      animation: 150,
      handle: ".handle",
    });
  }
});
