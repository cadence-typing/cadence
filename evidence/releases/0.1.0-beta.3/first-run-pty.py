"""#78/#79 first-run and settings check in a real PTY on a temporary HOME."""
import os, sys, time, fcntl, termios, struct, tempfile, select
import pty as ptymod
import pyte

def launch(argv, env, cols=100, rows=32):
    screen = pyte.Screen(cols, rows); stream = pyte.ByteStream(screen)
    pid, fd = ptymod.fork()
    if pid == 0:
        os.execve(argv[0], argv, env)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    return pid, fd, screen, stream

def pump(fd, stream, seconds):
    end = time.time() + seconds
    while (left := end - time.time()) > 0:
        r, _, _ = select.select([fd], [], [], left)
        if r:
            try: data = os.read(fd, 65536)
            except OSError: return
            if not data: return
            stream.feed(data)

def text(screen): return "\n".join(l.rstrip() for l in screen.display)

binary, out = sys.argv[1], sys.argv[2]
home = tempfile.mkdtemp(prefix="home-", dir=os.path.dirname(out))
env = dict(os.environ, TERM="xterm-256color", HOME=home, XDG_CONFIG_HOME=os.path.join(home, "config"), XDG_DATA_HOME=os.path.join(home, "data"))
argv = [binary, "--utc-offset", "+00:00"]
frames = []
def run(keys, label):
    pid, fd, screen, stream = launch(argv, env)
    pump(fd, stream, 2.0)
    # The first frame needs a resize to render.
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 32, 100, 0, 0)); pump(fd, stream, 0.8)
    frames.append((label + " / at launch", text(screen)))
    for k, name in keys:
        os.write(fd, k); pump(fd, stream, 0.8)
        if name: frames.append((label + " / " + name, text(screen)))
    os.write(fd, b"q"); pump(fd, stream, 1.5)
    try: os.waitpid(pid, 0)
    except ChildProcessError: pass
RIGHT, LEFT, DOWN, ENTER, ESC = b"\x1b[C", b"\x1b[D", b"\x1b[B", b"\r", b"\x1b"
run([(RIGHT, None), (DOWN, None), (LEFT, "edited"), (ENTER, "after Enter")], "first run")
cfg = os.path.join(home, "config", "cadence", "config.toml")
config = open(cfg).read() if os.path.exists(cfg) else "(no file)"
run([(b"s", "settings open"), (ESC, None)], "second run")
with open(out, "w") as f:
    for label, frame in frames:
        f.write(f"===== {label}\n{frame}\n")
    f.write(f"===== config.toml\n{config}\n")
print(out)
