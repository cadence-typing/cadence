"""beta.3 evidence (#87): setup, settings, the four destinations and a pack
session from the real binary, at 60x18 and 100x32, on temporary HOMEs."""
import os, sys, time, fcntl, termios, struct, tempfile, select
import pty as ptymod
import pyte
binary, out = sys.argv[1], sys.argv[2]
def session(argv, cols, rows, steps, home):
    env = dict(os.environ, TERM="xterm-256color", HOME=home, XDG_CONFIG_HOME=os.path.join(home, "config"), XDG_DATA_HOME=os.path.join(home, "data"))
    screen = pyte.Screen(cols, rows); stream = pyte.ByteStream(screen)
    pid, fd = ptymod.fork()
    if pid == 0:
        os.execve(binary, [binary] + argv, env)
    def pump(s):
        end = time.time() + s
        while (left := end - time.time()) > 0:
            r, _, _ = select.select([fd], [], [], left)
            if r:
                try: data = os.read(fd, 65536)
                except OSError: return
                if not data: return
                stream.feed(data)
    for _ in range(2):
        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0)); pump(1.2)
    frames = []
    for keys, label in steps:
        os.write(fd, keys); pump(0.9)
        if label: frames.append((f"{label} @{cols}x{rows}", "\n".join(l.rstrip() for l in screen.display)))
    os.write(fd, b"q"); pump(1.0)
    try: os.waitpid(pid, 0)
    except ChildProcessError: pass
    return frames
NONE, ENTER, TAB, BACK, ESC = b"", b"\r", b"\t", b"\x1b[Z", b"\x1b"
frames = []
for cols, rows in [(60, 18), (100, 32)]:
    home = tempfile.mkdtemp(prefix="home-", dir=os.path.dirname(out))
    frames += session(["--utc-offset", "+00:00"], cols, rows, [
        (NONE, "first-run setup"), (ENTER, "Today"), (TAB, "Train"), (TAB, "Numpad"),
        (TAB, "Progress"), (BACK + BACK + BACK, None), (b"s", "settings"), (ESC, None)], home)
    frames += session(["--pack", "code-shell", "--minutes", "5", "--utc-offset", "+00:00"], cols, rows, [
        (ENTER, "pack session"), (b"\x03", "pack session review"), (ENTER, None)], home)
with open(out, "w") as f:
    for label, frame in frames:
        f.write(f"===== {label}\n{frame}\n")
print(len(frames), "frames")
