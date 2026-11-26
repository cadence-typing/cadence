"""Two full default sessions in a PTY (#53): type what is under the caret,
with scripted slips; capture the start screen, one exercise per phase, the
closing review and the results at 100x32 and 60x18."""
import os, sys, time, tempfile, fcntl, termios, struct, json, random
import pty as ptymod, select
import pyte

class Term:
    def __init__(self, argv, cols, rows):
        self.cols, self.rows = cols, rows
        self.screen = pyte.Screen(cols, rows)
        self.stream = pyte.ByteStream(self.screen)
        pid, fd = ptymod.fork()
        if pid == 0:
            e = dict(os.environ, TERM="xterm-256color")
            os.execve(argv[0], argv, e)
        self.pid, self.fd = pid, fd
        self._winsz()
    def _winsz(self):
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", self.rows, self.cols, 0, 0))
    def resize(self, cols, rows):
        self.cols, self.rows = cols, rows
        self.screen.resize(rows, cols)
        self._winsz()
        self.pump(0.6)
    def pump(self, seconds=0.2):
        end = time.time() + seconds
        while True:
            left = end - time.time()
            if left <= 0: break
            r, _, _ = select.select([self.fd], [], [], left)
            if r:
                try: data = os.read(self.fd, 65536)
                except OSError: return False
                if not data: return False
                self.stream.feed(data)
        return True
    def text(self):
        return "\n".join(line.rstrip() for line in self.screen.display)
    def send(self, s, pause=0.05):
        os.write(self.fd, s.encode() if isinstance(s, str) else s)
        self.pump(pause)
    def caret(self):
        for y in range(0, self.rows - 3):
            row = self.screen.buffer[y]
            for x in range(self.cols):
                if row[x].reverse:
                    rest = "".join(row[i].data for i in range(x + 1, self.cols)).strip()
                    return row[x].data, rest
        return None

BIG, SMALL = (100, 32), (60, 18)

def run(binary, profile, offset, label, slips, out):
    extra = os.environ.get("CADENCE_ARGS", "").split()
    t = Term([binary, "--profile-dir", profile, "--utc-offset", offset] + extra, *BIG)
    t.pump(2.0)
    frames = {}
    def both(name):
        frames[name + " @100x32"] = t.text()
        t.resize(*SMALL)
        frames[name + " @60x18"] = t.text()
        t.resize(*BIG)
        # A resize may pause practice; resume.
        if " PAUSED " in t.text():
            t.send("\r", 0.5)
    both("start")
    t.send("\r", 1.0)
    rng = random.Random(7)
    seen_phase = set()
    deadline = time.time() + float(os.environ.get("RUN_SECONDS", 20 * 60))
    typed_in_exercise = 0
    last_log = 0
    log = open(out + ".log", "w")
    while time.time() < deadline:
        screen = t.text()
        if time.time() - last_log > 15:
            last_log = time.time()
            log.write(f"--- {time.strftime('%H:%M:%S')} caret={t.caret()!r}\n{screen}\n")
            log.flush()
        if "Session review" in screen and "Save & exit" in screen:
            both("results")
            t.send("\r", 2.0)
            break
        if "UP NEXT" in screen:
            t.pump(1.0)
            t.send("\r", 0.8)
            typed_in_exercise = 0
            continue
        if " PAUSED " in screen:
            t.send("\r", 0.5)
            continue
        # Phase name from the route line: the active (●) phase.
        head = screen.splitlines()[0] if screen else ""
        phase = head.split("●")[1].split()[0] if "●" in head else "?"
        c = t.caret()
        if c is None:
            t.pump(0.2)
            continue
        ch, rest = c
        if phase not in seen_phase and typed_in_exercise == 3:
            seen_phase.add(phase)
            both("phase " + phase)
        if ch == " " and not rest and "Enter submits" in screen:
            t.send("\r", 0.15)
            typed_in_exercise = 0
            continue
        if ch == " " and not rest:
            ch = " "
        # Scripted slips: a wrong key then Backspace, at a rate per session.
        if ch.strip() and rng.random() < slips:
            wrong = "x" if ch != "x" else "q"
            if ch.isdigit():
                wrong = "5" if ch != "5" else "6"
            t.send(wrong, 0.05)
            t.send("\x7f", 0.05)
        t.send(ch if ch != "␣" else " ", 0.09)
        typed_in_exercise += 1
    with open(out, "w") as f:
        for name, frame in frames.items():
            f.write("=" * 20 + f" {label}: {name} " + "=" * 20 + "\n" + frame + "\n")
    # Give the final save time, then make sure the app is gone.
    for _ in range(50):
        pid, _status = os.waitpid(t.pid, os.WNOHANG)
        if pid:
            break
        t.pump(0.2)
    else:
        import signal
        os.kill(t.pid, signal.SIGTERM)
        os.waitpid(t.pid, 0)

if __name__ == "__main__":
    binary, profile = sys.argv[1], sys.argv[2]
    if os.environ.get("SMOKE"):
        run(binary, profile, "-12:00", "smoke", 0.06, sys.argv[3] + "/smoke.txt")
        sys.exit(0)
    run(binary, profile, "-12:00", "session 1", 0.06, sys.argv[3] + "/session1.txt")
    run(binary, profile, "+14:00", "session 2", 0.03, sys.argv[3] + "/session2.txt")
    print("done")
