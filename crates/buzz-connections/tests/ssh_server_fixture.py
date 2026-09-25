"""Isolated SSH protocol fixture for the real Windows OpenSSH client.
Never launches shell commands; accepts only disposable test credentials.
"""
import socket
import sys
import threading
import time
import paramiko

host_key = paramiko.RSAKey.generate(2048)
public_key = open(sys.argv[1], encoding="utf-8").read().split()[1]

class Server(paramiko.ServerInterface):
    def __init__(self):
        self.executed = threading.Event()
    def get_allowed_auths(self, username):
        return "publickey,password"
    def check_auth_password(self, username, password):
        return paramiko.AUTH_SUCCESSFUL if username == "fixture" and password == "fixture-password" else paramiko.AUTH_FAILED
    def check_auth_publickey(self, username, key):
        return paramiko.AUTH_SUCCESSFUL if username == "fixture" and key.get_base64() == public_key else paramiko.AUTH_FAILED
    def check_channel_request(self, kind, channel_id):
        return paramiko.OPEN_SUCCEEDED if kind == "session" else paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED
    def check_channel_exec_request(self, channel, command):
        if command != b"fixture-command":
            return False
        self.executed.set()
        return True

def handle(client):
    transport = paramiko.Transport(client)
    try:
        transport.add_server_key(host_key)
        server = Server()
        transport.start_server(server=server)
        channel = transport.accept(15)
        if channel is not None and server.executed.wait(10):
            channel.sendall(b"BUZZ_WINDOWS_SSH_OK")
            channel.send_exit_status(0)
            channel.shutdown_write()
            channel.close()
            # Let OpenSSH consume exit-status/EOF and disconnect before closing TCP.
            # A fixed short sleep can reset the Windows client before it reads them.
            deadline = time.monotonic() + 10
            while transport.is_active() and time.monotonic() < deadline:
                time.sleep(0.02)
    except (EOFError, OSError, paramiko.SSHException):
        pass
    finally:
        transport.close()

listener = socket.socket()
listener.bind(("127.0.0.1", 0))
listener.listen(8)
print(listener.getsockname()[1], flush=True)
for _ in range(16):
    client, _address = listener.accept()
    threading.Thread(target=handle, args=(client,), daemon=True).start()
