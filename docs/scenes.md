# Scene files

[Русский ниже](#русский)

A scene is a JSON file. Three scenes are built in (`rust/scenes/classic.json`, `sunroom.json`, `materials.json`: copy one as a starting point).
Put your own `.json` files here and pick them on the **Scene** tab:

- Android: `/sdcard/Android/data/dev.starinin.raytrace/files/scenes/`
- Linux: `~/.config/raytrace/scenes/` (honours `XDG_CONFIG_HOME`)

A file with an error is not loaded; the reason is shown (a toast on Android, red text in the Linux panel).

```json
{
  "name": "My scene",
  "camera": {"target": [0, 1.2, 0], "yaw": 0.3, "pitch": 0.35, "dist": 8, "dist_range": [3, 14],
             "orbit_min": [-5, 0.3, -6], "orbit_max": [5, 10, 9], "fly_min": [-5, 0.3, -6], "fly_max": [5, 8, 9]},
  "sun":   {"dir": [0.3, 1, 0.2], "color": [3, 2.7, 2.2], "angle_deg": 1.2},
  "lamps": [{"pos": [0, 4, 0], "radius": 0.5, "color": [30, 30, 30]}, {"pos": [3, 2, 0], "radius": 0.4, "user": "b"}],
  "materials": [{"name": "wall", "color": [0.8, 0.8, 0.75]}],
  "objects": [{"type": "box", "pos": [0, -0.5, 0], "size": [6, 0.5, 6], "material": "wall"}]
}
```

- `camera`: orbit target and start pose (`yaw`, `pitch`, `dist`), the allowed distance range and the position limits for the orbit and fly cameras. Everything is optional (defaults = the classic scene).
- `sun` (optional): `dir` points **towards** the sun. `color` is the radiance of a white diffuse surface facing the sun (about 3 is bright daylight). `angle_deg` is the sun's apparent size (bigger = softer shadows). The Sky setting scales it: Day full, Dusk dim orange, Night off. The *Light intensity* setting scales it too.
- `lamps` (at most 2): emissive spheres that light the scene. `color` is fixed; `"user": "a"` or `"b"` uses the *Light A / B colour* setting instead. Give each lamp a visible object with an `emissive` material too.
- `materials` (at most 16), referenced by `name`:
  - `kind`: `diffuse` (default), `metal` (`color` = tint, `rough` 0..1), `glass` (`color` = tint, clear = white), `emissive` (`emit` or `user`), `glossy` (diffuse under a specular coat)
  - `checker` + `checker_scale`: a second colour and the tile size for a floor checkerboard (in world x/z)
- `objects`: at most 16 spheres and boxes, plus at most one each of `menger`, `torus` and `blobs`:
  - `sphere`: `pos`, `radius`
  - `box`: `pos`, `size` (half extents), optional `round` (corner radius), `spin` (rad/s about the vertical axis)
  - `torus`: `pos`, `radii` [major, minor]; `menger`: `pos`, `size` (half size), `spin`; `blobs`: `pos`, `pos2`, `radii` [r1, r2], `smooth`
  - thin **glass boxes** act as coloured filters: sunlight and lamp light passing through them is tinted, so they cast coloured light. Glass spheres block direct light; their focused light (caustic) is added for the first glass sphere.

Walls are just boxes; leave a gap (for example two ceiling boxes with a slit between them) to let the sun in.

## Русский

Сцена — JSON-файл. Три сцены встроены (`rust/scenes/classic.json`, `sunroom.json`, `materials.json`: скопируйте любую как основу).
Свои `.json` кладите сюда и выбирайте на вкладке **«Сцена»**:

- Android: `/sdcard/Android/data/dev.starinin.raytrace/files/scenes/`
- Linux: `~/.config/raytrace/scenes/` (учитывает `XDG_CONFIG_HOME`)

Файл с ошибкой не загружается, причина показывается (уведомление на Android, красный текст в панели Linux).

Поля те же, что в примере выше:

- `camera`: цель орбиты и стартовая поза (`yaw`, `pitch`, `dist`), диапазон расстояния и границы позиции для орбитальной и полётной камеры. Всё необязательно (по умолчанию — классическая сцена).
- `sun` (необязательно): `dir` указывает **на** солнце. `color` — яркость белой матовой поверхности, обращённой к солнцу (около 3 — яркий день). `angle_deg` — видимый размер солнца (больше — мягче тени). Настройка «Небо» масштабирует его: день — полностью, закат — тускло-оранжевое, ночь — выключено. «Яркость ламп» тоже масштабирует.
- `lamps` (до 2): излучающие сферы, освещающие сцену. `color` фиксированный; `"user": "a"` или `"b"` берёт цвет из настройки «Цвет лампы A / B». Добавьте каждой лампе видимый объект с материалом `emissive`.
- `materials` (до 16), ссылки по `name`:
  - `kind`: `diffuse` (по умолчанию), `metal` (`color` — оттенок, `rough` 0..1), `glass` (`color` — оттенок, прозрачное = белое), `emissive` (`emit` или `user`), `glossy` (матовая основа под глянцевым слоем)
  - `checker` + `checker_scale`: второй цвет и размер клетки шахматного пола (в мировых x/z)
- `objects`: до 16 сфер и боксов, плюс не более одного `menger`, `torus` и `blobs`:
  - `sphere`: `pos`, `radius`; `box`: `pos`, `size` (половинные размеры), `round` (скругление), `spin` (рад/с вокруг вертикали)
  - `torus`: `pos`, `radii` [большой, малый]; `menger`: `pos`, `size` (полуразмер), `spin`; `blobs`: `pos`, `pos2`, `radii` [r1, r2], `smooth`
  - тонкие **стеклянные боксы** работают как цветные фильтры: солнечный и ламповый свет, прошедший через них, окрашивается, поэтому они отбрасывают цветной свет. Стеклянные сферы блокируют прямой свет; их фокусированный свет (каустика) добавляется для первой стеклянной сферы.

Стены — это просто боксы; оставьте щель (например, два бокса потолка с зазором), чтобы впустить солнце.
