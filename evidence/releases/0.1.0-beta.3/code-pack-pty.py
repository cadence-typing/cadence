import os, sys, time, fcntl, termios, struct, tempfile, select, sqlite3, glob
import pty as ptymod
import pyte
binary, out = sys.argv[1], sys.argv[2]
home = tempfile.mkdtemp(prefix="home-", dir=os.path.dirname(out))
os.makedirs(os.path.join(home, "config", "cadence"))
open(os.path.join(home, "config", "cadence", "config.toml"), "w").write('[keyboard]\nlayout = "us"\n')
env = dict(os.environ, TERM="xterm-256color", HOME=home, XDG_CONFIG_HOME=os.path.join(home, "config"), XDG_DATA_HOME=os.path.join(home, "data"))
screen = pyte.Screen(100, 32); stream = pyte.ByteStream(screen)
pid, fd = ptymod.fork()
if pid == 0:
    os.execve(binary, [binary, "--pack", "code-rust", "--minutes", "5", "--utc-offset", "+00:00"], env)
def pump(s):
    end = time.time() + s
    while (left := end - time.time()) > 0:
        r, _, _ = select.select([fd], [], [], left)
        if r:
            try: data = os.read(fd, 65536)
            except OSError: return
            if not data: return
            stream.feed(data)
def text(): return "\n".join(l.rstrip() for l in screen.display)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 32, 100, 0, 0)); pump(2.0)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 32, 100, 0, 0)); pump(0.8)
frames = [("start", text())]
os.write(fd, b"\r"); pump(1.0); frames.append(("practice", text()))
os.write(fd, b"\x03"); pump(1.5); frames.append(("review", text()))
os.write(fd, b"\r"); pump(2.0)
try: os.waitpid(pid, 0)
except ChildProcessError: pass
db = glob.glob(os.path.join(home, "data", "cadence", "progress.sqlite3"))
rows = sqlite3.connect(db[0]).execute("select session_id, content_class, length(pack_digest), status from sessions").fetchall() if db else "no db"
with open(out, "w") as f:
    for label, frame in frames: f.write(f"===== {label}\n{frame}\n")
    f.write(f"===== sessions\n{rows}\n")
