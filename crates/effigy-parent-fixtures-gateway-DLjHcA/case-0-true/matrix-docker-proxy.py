import pathlib,socket,sys
server=socket.socket()
server.bind(('127.0.0.1',0))
server.listen()
report=pathlib.Path(sys.argv[1])
pending=report.with_suffix('.next')
pending.write_text('127.0.0.1:'+str(server.getsockname()[1]))
pending.replace(report)
while True:
 connection,_=server.accept()
 with connection:
  connection.settimeout(1)
  try:
   connection.recv(4096)
   connection.sendall(b'HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: close\r\n\r\nmatrix proxy')
  except OSError:
   pass
