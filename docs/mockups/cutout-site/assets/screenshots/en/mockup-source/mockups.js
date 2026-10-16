(() => {
  const validScenes = new Set(["rgb", "tune", "ride-tracking"]);
  const requested = new URLSearchParams(window.location.search).get("scene");
  const scene = validScenes.has(requested) ? requested : "rgb";
  document.body.dataset.scene = scene;
  document.getElementById("navigation-title").textContent =
    scene === "rgb" ? "Lighting" : scene === "tune" ? "Tune" : "Map";
})();
