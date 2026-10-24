#!/usr/bin/env python3
"""essais/cite.py AVENTURE.pak "ÉTAPES" [CAPTURE.png] : bâtit par script une cité type
sur la carte du premier épisode d'une aventure, puis joue les étapes données (par
exemple "t:19200 bilan maisons").

La cité occupe un rectangle de terrain libre de 34 × 24 cases : trois rues bouclées,
des logements de part et d'autre, services et agora au nord, ateliers au sud, palais à
l'est. Seuls les bâtiments que l'épisode permet sont posés. Les vivres viennent d'une
laiterie, sinon de fermes sur le pré, sinon de pêcheries reliées par une route au
rivage le plus proche. Il faut un affichage : Xvfb et DISPLAY=:99 font l'affaire."""

import collections
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GRID = 228
WIDTH, HEIGHT = 34, 24
FREE = '.:'


def olympe(adventure, script, capture='/dev/null'):
    command = [os.path.join(ROOT, 'target/release/olympe'), '--donnees', os.path.join(ROOT, 'donnees'), '--sans-intro',
               '--ticks', '0', '--capture', capture, '--script', script, adventure]
    env = dict(os.environ, OLYMPE_MUET='1')
    return subprocess.run(command, capture_output=True, text=True, env=env, cwd=ROOT)


def survey(adventure):
    """Plan du terrain (lignes de caractères) et bâtiments permis."""
    out = olympe(adventure, f'carte:0,0,{GRID - 1},{GRID - 1} types').stdout
    rows, allowed = {}, set()
    for line in out.splitlines():
        head = line[:5].strip()
        if head.isdigit() and len(line) > 5:
            rows[int(head)] = line[5:]
        elif '×' in line and 'interdit' not in line:
            allowed.add(line.split(' (')[0])
    grid = [rows.get(y, ' ' * GRID).ljust(GRID) for y in range(GRID)]
    return grid, allowed


def free_rect(grid, x, y, w, h):
    return all(grid[y + j][x + i] in FREE for j in range(h) for i in range(w))


def choose_site(grid, want_water, want_meadow=False):
    # Sommes cumulées des cases libres, pour tester chaque rectangle d'un coup.
    acc = [[0] * (GRID + 1) for _ in range(GRID + 1)]
    for y in range(GRID):
        for x in range(GRID):
            acc[y + 1][x + 1] = acc[y][x + 1] + acc[y + 1][x] - acc[y][x] + (grid[y][x] in FREE)
    water = [(x, y) for y in range(0, GRID, 2) for x in range(0, GRID, 2) if grid[y][x] == '~']
    best = None
    for y in range(1, GRID - HEIGHT - 1):
        for x in range(1, GRID - WIDTH - 1):
            free = acc[y + HEIGHT][x + WIDTH] - acc[y][x + WIDTH] - acc[y + HEIGHT][x] + acc[y][x]
            # Un terrain libre à 90 % suffit : ce qui ne tient pas est refusé.
            if free * 10 < WIDTH * HEIGHT * 9:
                continue
            cx, cy = x + WIDTH // 2, y + HEIGHT // 2
            if want_water:
                score = min((abs(wx - cx) + abs(wy - cy) for wx, wy in water), default=999)
            else:
                score = abs(cx - GRID // 2) + abs(cy - GRID // 2)
            if want_meadow:
                # Fermes au sud de la dernière rue : autant de prés que possible.
                score -= 1000 * len(meadow_slots(grid, x, y))
            score += 20 * (WIDTH * HEIGHT - free)
            if best is None or score < best[0]:
                best = (score, x, y)
    return best and best[1:]


def meadow_slots(grid, x0, y0):
    return [dx for dx in range(1, 25, 3) if all(grid[y0 + 20 + j][x0 + dx + i] == ':' for i in range(3) for j in range(3))]


def route_to(grid, start, taken, fits):
    """Route depuis `start` jusqu'à un emplacement 3 × 3 qui convient (`fits`) : rend
    (chemin, coin de l'emplacement)."""
    previous = {start: None}
    queue = collections.deque([start])
    while queue:
        x, y = queue.popleft()
        for fx, fy in [(x + 1, y - 1), (x - 3, y - 1), (x - 1, y + 1), (x - 1, y - 3)]:
            footprint = [(fx + i, fy + j) for i in range(3) for j in range(3)]
            if all(0 <= tx < GRID and 0 <= ty < GRID and (tx, ty) not in taken and (tx, ty) not in previous for tx, ty in footprint) and fits(fx, fy):
                path = []
                at = (x, y)
                while at:
                    path.append(at)
                    at = previous[at]
                return path, (fx, fy)
        for nx, ny in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]:
            if 0 <= nx < GRID and 0 <= ny < GRID and (nx, ny) not in previous and grid[ny][nx] in FREE + '=' and (nx, ny) not in taken:
                previous[(nx, ny)] = (x, y)
                queue.append((nx, ny))
    return None, None


def shore(grid):
    """Emplacement de pêcherie : libre, au bord de l'eau."""
    def fits(x, y):
        ring = [(x + i, y + j) for i in range(-1, 4) for j in range(-1, 4)]
        return all(grid[y + j][x + i] in FREE for i in range(3) for j in range(3)) and any(
            0 <= rx < GRID and 0 <= ry < GRID and grid[ry][rx] == '~' for rx, ry in ring)
    return fits


def meadow(grid):
    """Emplacement de ferme : tout en pré."""
    return lambda x, y: all(grid[y + j][x + i] == ':' for i in range(3) for j in range(3))


def plan(grid, allowed):
    def ok(name):
        return name in allowed

    food = 'Laiterie' if ok('Laiterie') else next((f for f in ['Exploitation de blé', 'Exploitation de carottes', "Exploitation d'oignons"] if ok(f)), None)
    if food is None and ok('Pêcherie'):
        food = 'Pêcherie'
    site = choose_site(grid, food == 'Pêcherie', bool(food) and food.startswith('Exploitation'))
    if site is None:
        sys.exit('aucun terrain de 34 × 24 cases libre à 90 %')
    x0, y0 = site
    steps = []

    def road(x1, y1, x2, y2):
        steps.append(f'r:{x0 + x1},{y0 + y1},{x0 + x2},{y0 + y2}')

    def put(dx, dy, name):
        if ok(name):
            steps.append(f'b:{x0 + dx},{y0 + dy},{name.replace(" ", "_")}')

    for row in (3, 11, 19):
        road(0, row, 27, row)
    road(0, 3, 0, 19)
    road(27, 3, 27, 19)
    # Nord : agora, services, culture.
    put(1, 1, 'Agora')
    for dx, vendor in [(1, 'Epicière'), (3, 'Marchand de laine'), (5, "Marchand d'huile")]:
        put(dx, 1, vendor)
    for dx, name in [(7, 'Fontaine'), (9, 'Maintenance'), (11, 'Hôtel des impôts'), (13, 'Podium'), (21, 'Fontaine'), (23, 'Maintenance'), (25, 'Poste de garde')]:
        put(dx, 1, name)
    put(15, 0, 'Gymnase')
    put(18, 0, 'Collège')
    # Logements des deux rues résidentielles, jardins entre elles.
    for dx in range(1, 27, 2):
        put(dx, 4, 'Logement modeste')
        put(dx, 6, "Jardin d'agrément")
        put(dx, 8, 'Parc')
    # Seconde rue : une agora pour ses maisons.
    put(21, 9, 'Agora')
    for dx, vendor in [(21, 'Epicière'), (23, 'Marchand de laine'), (25, "Marchand d'huile")]:
        put(dx, 9, vendor)
    for dx in range(1, 27, 2):
        if dx < 21:
            put(dx, 9, {7: 'Maintenance', 13: 'Fontaine', 19: 'Podium'}.get(dx, 'Logement modeste'))
        put(dx, 12, 'Logement modeste')
    # Sud : grenier, entrepôt, ateliers.
    put(1, 15, 'Grenier')
    put(5, 16, 'Entrepôt')
    for dx, name in [(8, 'Bergerie'), (10, 'Bergerie'), (12, 'Bergerie'), (14, 'Métairie'), (16, 'Pressoir à olives'), (18, 'Maintenance'), (20, 'Fontaine')]:
        put(dx, 17, name)
    if food == 'Laiterie':
        for dx in (22, 24):
            put(dx, 17, 'Laiterie')
    elif food and food.startswith('Exploitation'):
        for dx in meadow_slots(grid, x0, y0):
            put(dx, 20, food)
    put(28, 3, 'Palais')
    outside = {'Pêcherie': (shore(grid), 2), 'Exploitation': (meadow(grid), 4)}.get(food and food.split(' ')[0])
    if outside and not (food.startswith('Exploitation') and len(meadow_slots(grid, x0, y0)) >= 3):
        fits, count = outside
        # Les vivres passent avant le reste, quand l'argent est compté.
        city, steps = steps, []
        taken = {(x0 + i, y0 + j) for i in range(WIDTH) for j in range(HEIGHT)}
        for _ in range(count):
            path, corner = route_to(grid, (x0 + 13, y0 + 19), taken - {(x0 + i, y0 + 19) for i in range(28)}, fits)
            if not path:
                break
            for (ax, ay), (bx, by) in zip(path, path[1:]):
                steps.append(f'r:{ax},{ay},{bx},{by}')
            steps.append(f'b:{corner[0]},{corner[1]},{food.replace(" ", "_")}')
            taken |= {(corner[0] + i, corner[1] + j) for i in range(3) for j in range(3)}
            # Une maintenance au bord de la route, près de l'exploitation : loin de la
            # cité, aucun surintendant ne passerait.
            road = set(path)
            for px, py in path[:12]:
                spot = next(((px + dx, py + dy) for dx, dy in [(1, 0), (-2, 0), (0, 1), (0, -2), (1, -1), (-2, -1), (-1, 1), (-1, -2)]
                             if all(0 <= px + dx + i < GRID and 0 <= py + dy + j < GRID and grid[py + dy + j][px + dx + i] in FREE
                                    and (px + dx + i, py + dy + j) not in taken | road for i in range(2) for j in range(2))), None)
                if spot:
                    steps.append(f'b:{spot[0]},{spot[1]},Maintenance')
                    taken |= {(spot[0] + i, spot[1] + j) for i in range(2) for j in range(2)}
                    break
        steps = city[:5] + steps + city[5:]
    return steps, (x0, y0), food


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    adventure, extra = os.path.abspath(sys.argv[1]), sys.argv[2]
    capture = os.path.abspath(sys.argv[3]) if len(sys.argv) > 3 else '/dev/null'
    grid, allowed = survey(adventure)
    steps, site, food = plan(grid, allowed)
    print(f'cité en {site}, vivres : {food}')
    out = olympe(adventure, ' '.join(steps + [extra]), capture)
    for line in (out.stdout + out.stderr).splitlines():
        if not line.startswith(('sauvegarde', 'après 0', 'script terminé', 'carte de')):
            print(line)


main()
