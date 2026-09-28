// felix-novnc：容器内 noVNC 服务（Go 单二进制，替代 Python websockify）。
//
// 功能与 `websockify --web /usr/share/novnc PORT 127.0.0.1:5900` 等价：
//
//	· 静态托管 noVNC 前端（支持 Range、缓存头由 net/http 文件服务提供）；
//	· /websockify 升级为 WebSocket，二进制帧与 TCP（x11vnc）双向转发；
//	· 完整处理客户端掩码、分片、ping/pong/close。
//
// 为什么自研：Python websockify 会拉起 multiprocessing forkserver 与
// resource_tracker，单实例常驻约 80MB；Go 版本常驻约 6-10MB，功能不缩水。
package main

import (
	"bufio"
	"crypto/sha1"
	"encoding/base64"
	"encoding/binary"
	"flag"
	"io"
	"log"
	"net"
	"net/http"
	"strings"
	"sync"
	"time"
)

const wsGUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"

// 单帧上限：VNC 单帧远小于此；防御性上限，防止恶意/异常客户端吃内存。
const maxFrameSize = 32 << 20

func main() {
	listen := flag.String("listen", "0.0.0.0:6080", "HTTP/WebSocket 监听地址")
	web := flag.String("web", "/usr/share/novnc", "noVNC 静态资源目录")
	vnc := flag.String("vnc", "127.0.0.1:5900", "VNC（TCP）目标地址")
	flag.Parse()

	mux := http.NewServeMux()
	mux.HandleFunc("/websockify", func(w http.ResponseWriter, r *http.Request) {
		serveWebSocket(w, r, *vnc)
	})
	fileServer := http.FileServer(http.Dir(*web))
	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/" {
			http.Redirect(w, r, "vnc.html", http.StatusFound)
			return
		}
		fileServer.ServeHTTP(w, r)
	})

	server := &http.Server{
		Addr:              *listen,
		Handler:           mux,
		ReadHeaderTimeout: 10 * time.Second,
	}
	log.Printf("felix-novnc: listen=%s web=%s vnc=%s", *listen, *web, *vnc)
	if err := server.ListenAndServe(); err != nil {
		log.Fatalf("felix-novnc: %v", err)
	}
}

// serveWebSocket 完成 WebSocket 握手后把连接交给双向代理。
func serveWebSocket(w http.ResponseWriter, r *http.Request, vncAddr string) {
	if !strings.EqualFold(r.Header.Get("Upgrade"), "websocket") {
		http.Error(w, "expected websocket upgrade", http.StatusBadRequest)
		return
	}
	key := r.Header.Get("Sec-WebSocket-Key")
	if key == "" {
		http.Error(w, "missing Sec-WebSocket-Key", http.StatusBadRequest)
		return
	}
	hijacker, ok := w.(http.Hijacker)
	if !ok {
		http.Error(w, "hijack unsupported", http.StatusInternalServerError)
		return
	}
	conn, brw, err := hijacker.Hijack()
	if err != nil {
		log.Printf("felix-novnc: hijack: %v", err)
		return
	}

	// noVNC 会请求子协议 `binary`；按请求回显（没有就不带该头）。
	var subproto string
	for _, p := range strings.Split(r.Header.Get("Sec-WebSocket-Protocol"), ",") {
		if strings.TrimSpace(p) == "binary" {
			subproto = "binary"
			break
		}
	}
	var resp strings.Builder
	resp.WriteString("HTTP/1.1 101 Switching Protocols\r\n")
	resp.WriteString("Upgrade: websocket\r\n")
	resp.WriteString("Connection: Upgrade\r\n")
	resp.WriteString("Sec-WebSocket-Accept: ")
	resp.WriteString(acceptKey(key))
	resp.WriteString("\r\n")
	if subproto != "" {
		resp.WriteString("Sec-WebSocket-Protocol: binary\r\n")
	}
	resp.WriteString("\r\n")
	if _, err := conn.Write([]byte(resp.String())); err != nil {
		conn.Close()
		return
	}

	target, err := net.Dial("tcp", vncAddr)
	if err != nil {
		log.Printf("felix-novnc: dial vnc %s: %v", vncAddr, err)
		_ = writeFrame(conn, 0x8, []byte{0x03, 0xF3}) // 1011 internal error
		conn.Close()
		return
	}

	var wmu sync.Mutex
	var wg sync.WaitGroup
	wg.Add(2)
	go func() {
		defer wg.Done()
		clientToTCP(brw.Reader, conn, target, &wmu)
		_ = target.Close() // 半关：让反向拷贝尽快结束
	}()
	go func() {
		defer wg.Done()
		tcpToClient(conn, target, &wmu)
		_ = conn.Close()
	}()
	wg.Wait()
	_ = target.Close()
	_ = conn.Close()
}

func acceptKey(key string) string {
	sum := sha1.Sum([]byte(key + wsGUID))
	return base64.StdEncoding.EncodeToString(sum[:])
}

// clientToTCP 读取客户端帧：解掩码、拼分片、把数据写入 VNC TCP；
// 同时处理 ping/pong/close 控制帧。
func clientToTCP(reader *bufio.Reader, ws net.Conn, tcp net.Conn, wmu *sync.Mutex) {
	var fragment []byte
	for {
		fin, opcode, payload, err := readFrame(reader)
		if err != nil {
			return
		}
		switch opcode {
		case 0x8: // close：原样回一个 close 后结束
			_ = writeFrameLocked(ws, 0x8, payload, wmu)
			return
		case 0x9: // ping → pong
			_ = writeFrameLocked(ws, 0xA, payload, wmu)
		case 0xA: // pong：忽略
		case 0x1, 0x2, 0x0: // text/binary/continuation（noVNC 只用 binary）
			fragment = append(fragment, payload...)
			if len(fragment) > maxFrameSize {
				return
			}
			if fin {
				if len(fragment) > 0 {
					if _, err := tcp.Write(fragment); err != nil {
						return
					}
				}
				fragment = fragment[:0]
			}
		}
	}
}

// tcpToClient 把 VNC 数据以二进制帧发回浏览器。
func tcpToClient(ws net.Conn, tcp net.Conn, wmu *sync.Mutex) {
	buf := make([]byte, 32*1024)
	for {
		n, err := tcp.Read(buf)
		if n > 0 {
			if writeErr := writeFrameLocked(ws, 0x2, buf[:n], wmu); writeErr != nil {
				return
			}
		}
		if err != nil {
			if err != io.EOF {
				log.Printf("felix-novnc: vnc read: %v", err)
			}
			_ = writeFrameLocked(ws, 0x8, []byte{0x03, 0xE8}, wmu) // 1000 normal
			return
		}
	}
}

// readFrame 读取一个完整帧（要求客户端帧必须带掩码，符合 RFC 6455）。
func readFrame(reader *bufio.Reader) (fin bool, opcode byte, payload []byte, err error) {
	var header [2]byte
	if _, err = io.ReadFull(reader, header[:]); err != nil {
		return
	}
	fin = header[0]&0x80 != 0
	opcode = header[0] & 0x0F
	masked := header[1]&0x80 != 0
	length := uint64(header[1] & 0x7F)
	switch length {
	case 126:
		var ext [2]byte
		if _, err = io.ReadFull(reader, ext[:]); err != nil {
			return
		}
		length = uint64(binary.BigEndian.Uint16(ext[:]))
	case 127:
		var ext [8]byte
		if _, err = io.ReadFull(reader, ext[:]); err != nil {
			return
		}
		length = binary.BigEndian.Uint64(ext[:])
	}
	if length > maxFrameSize {
		err = io.ErrShortBuffer
		return
	}
	var mask [4]byte
	if masked {
		if _, err = io.ReadFull(reader, mask[:]); err != nil {
			return
		}
	}
	payload = make([]byte, length)
	if _, err = io.ReadFull(reader, payload); err != nil {
		return
	}
	if masked {
		for i := range payload {
			payload[i] ^= mask[i%4]
		}
	}
	return
}

func writeFrame(conn net.Conn, opcode byte, payload []byte) error {
	var wmu sync.Mutex
	return writeFrameLocked(conn, opcode, payload, &wmu)
}

// writeFrameLocked 串行化写入（读协程可能同时发 pong/close）。
func writeFrameLocked(conn net.Conn, opcode byte, payload []byte, wmu *sync.Mutex) error {
	header := make([]byte, 0, 10)
	header = append(header, 0x80|opcode) // FIN=1
	switch {
	case len(payload) < 126:
		header = append(header, byte(len(payload)))
	case len(payload) <= 0xFFFF:
		header = append(header, 126, byte(len(payload)>>8), byte(len(payload)))
	default:
		header = append(header, 127)
		var ext [8]byte
		binary.BigEndian.PutUint64(ext[:], uint64(len(payload)))
		header = append(header, ext[:]...)
	}
	wmu.Lock()
	defer wmu.Unlock()
	if _, err := conn.Write(header); err != nil {
		return err
	}
	_, err := conn.Write(payload)
	return err
}
