(() => {
  const validScenes = new Set(["tune", "ride-tracking"]);
  const requested = new URLSearchParams(window.location.search).get("scene");
  const scene = validScenes.has(requested) ? requested : "tune";
  document.body.dataset.scene = scene;
  document.getElementById("navigation-title").textContent = scene === "tune" ? "Tune" : "Map";
})();
