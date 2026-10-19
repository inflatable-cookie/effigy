"""Report direct bind evidence without changing interface or resolver state."""
import os
import socket
import errno
import json

assert os.geteuid() != 0, "bind qualification must run unprivileged"
results = []
for address, port in [("127.0.0.1", 80), ("127.0.0.1", 81), ("127.0.0.1", 443), ("127.1.0.1", 5432), ("127.1.0.1", 80)]:
    with socket.socket() as listener:
        try:
            listener.bind((address, port))
            result = {"address": address, "port": port, "bound": True}
        except OSError as error:
            result = {"address": address, "port": port, "bound": False, "errno": error.errno, "reason": str(error)}
        results.append(result)
print(json.dumps({"uid": os.geteuid(), "results": results}, indent=2))
