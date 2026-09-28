// felix-llm-relay：OpenAI 兼容的模型请求中转（Go 单二进制，纯标准库）。
//
// 定位：DSH 内部契约（插件/会话/设置）每周都在变，不适合原生重写；而
// 「OpenAI 兼容 HTTP + SSE + Bearer 鉴权」是已经固化的行业协议，几乎不会变。
// 把这个边界放到原生进程里，DSH 侧只把它当普通的「自定义 OpenAI 提供商」：
//
//	设置 → 模型 → 添加提供商 → Base URL: http://127.0.0.1:3160/v1
//
// 能力（全部是协议已固化的部分）：
//
//	· GET  /v1/models           聚合声明的模型列表；
//	· POST /v1/chat/completions 按 model 路由到上游，SSE 流式原样透传；
//	· 其它 /v1/*（embeddings 等）按 body 里的 model 路由后同样透传；
//	· GET  /healthz             健康检查。
//
// 价值：上游真实 API Key 只存在本容器的配置文件里（0600），不进 DSH 的
// 设置/凭据存储；换厂商、加模型、做重试/超时都在这一个小二进制里完成，
// 与 DSH 版本解耦。常驻约 5MB。
package main

import (
	"context"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"sync"
	"syscall"
	"time"
)

const (
	maxBodyBytes       = 64 << 20 // 请求体上限（长上下文对话可能较大）
	defaultListen      = "127.0.0.1:3160"
	upstreamTimeout    = 10 * time.Minute
	responseHdrTimeout = 60 * time.Second
	configPollInterval = 5 * time.Second
)

// Upstream 一个 OpenAI 兼容的上游（DeepSeek 官方、OpenAI、Anthropic 的
// OpenAI 兼容端点、各类网关都适用）。
type Upstream struct {
	Name    string   `json:"name"`
	BaseURL string   `json:"baseURL"`
	APIKey  string   `json:"apiKey"`
	Models  []string `json:"models"`
	// Default 为 true 时承接未匹配到具体厂商的模型（可省略 Models 做纯透传）。
	Default bool `json:"default,omitempty"`
}

// Config 中转配置。首次启动会生成模板并落在数据卷里，用户直接编辑即可。
type Config struct {
	Listen    string     `json:"listen"`
	Upstreams []Upstream `json:"upstreams"`
}

var (
	cfgMu   sync.RWMutex
	cfg     Config
	cfgPath string
	cfgMod  time.Time
)

func main() {
	flag.StringVar(&cfgPath, "config", "", "配置文件路径（默认 $FELIX_RELAY_CONFIG 或 /data/llm-relay.json）")
	flag.Parse()
	if cfgPath == "" {
		cfgPath = os.Getenv("FELIX_RELAY_CONFIG")
	}
	if cfgPath == "" {
		cfgPath = "/data/llm-relay.json"
	}

	ensureConfigFile()
	if err := loadConfig(); err != nil {
		log.Fatalf("felix-llm-relay: 读取配置失败：%v", err)
	}

	mux := http.NewServeMux()
	mux.HandleFunc("/healthz", handleHealthz)
	mux.HandleFunc("/v1/models", handleModels)
	mux.HandleFunc("/v1/", handleProxy)

	server := &http.Server{
		Addr:              currentConfig().Listen,
		Handler:           mux,
		ReadHeaderTimeout: 15 * time.Second,
	}

	go watchConfig()

	// 收到 TERM/INT 时优雅退出（容器停启时由 dsh-entry 管理）
	ctx, stop := signal.NotifyContext(context.Background(), syscall.SIGTERM, syscall.SIGINT)
	defer stop()
	go func() {
		<-ctx.Done()
		shutdownCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		_ = server.Shutdown(shutdownCtx)
	}()

	log.Printf("felix-llm-relay: listen=%s upstreams=%d config=%s",
		currentConfig().Listen, len(currentConfig().Upstreams), cfgPath)
	if err := server.ListenAndServe(); err != nil && !errors.Is(err, http.ErrServerClosed) {
		log.Fatalf("felix-llm-relay: %v", err)
	}
}

// ensureConfigFile 首次启动生成模板（0600，含 listen 与空上游列表）。
func ensureConfigFile() {
	if _, err := os.Stat(cfgPath); err == nil {
		return
	}
	if err := os.MkdirAll(filepath.Dir(cfgPath), 0o700); err != nil {
		log.Printf("felix-llm-relay: 创建目录失败：%v", err)
		return
	}
	tmpl := Config{
		Listen: defaultListen,
		Upstreams: []Upstream{{
			Name:    "deepseek",
			BaseURL: "https://api.deepseek.com",
			APIKey:  "",
			Models:  []string{"deepseek-chat", "deepseek-reasoner"},
			Default: true,
		}},
	}
	data, _ := json.MarshalIndent(tmpl, "", "  ")
	data = append(data, '\n')
	if err := os.WriteFile(cfgPath, data, 0o600); err != nil {
		log.Printf("felix-llm-relay: 写配置模板失败：%v", err)
		return
	}
	log.Printf("felix-llm-relay: 已生成配置模板 %s —— 填入上游 baseURL/apiKey/models 后，在 DSH 里添加自定义提供商 http://127.0.0.1:3160/v1", cfgPath)
}

func loadConfig() error {
	info, err := os.Stat(cfgPath)
	if err != nil {
		return err
	}
	cfgMu.RLock()
	same := info.ModTime().Equal(cfgMod)
	cfgMu.RUnlock()
	if same {
		return nil
	}
	data, err := os.ReadFile(cfgPath)
	if err != nil {
		return err
	}
	var next Config
	if err := json.Unmarshal(data, &next); err != nil {
		return err
	}
	if next.Listen == "" {
		next.Listen = defaultListen
	}
	cfgMu.Lock()
	cfg = next
	cfgMod = info.ModTime()
	cfgMu.Unlock()
	return nil
}

func currentConfig() Config {
	cfgMu.RLock()
	defer cfgMu.RUnlock()
	return cfg
}

func watchConfig() {
	for range time.Tick(configPollInterval) {
		if err := loadConfig(); err != nil {
			log.Printf("felix-llm-relay: 配置重载失败（继续用旧配置）：%v", err)
		}
	}
}

func handleHealthz(w http.ResponseWriter, _ *http.Request) {
	cfg := currentConfig()
	writeJSON(w, http.StatusOK, map[string]any{
		"ok":        true,
		"upstreams": len(cfg.Upstreams),
	})
}

func handleModels(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		writeOpenAIError(w, http.StatusMethodNotAllowed, "仅支持 GET")
		return
	}
	type model struct {
		ID      string `json:"id"`
		Object  string `json:"object"`
		OwnedBy string `json:"owned_by"`
	}
	seen := map[string]bool{}
	list := make([]model, 0, 16)
	for _, up := range currentConfig().Upstreams {
		for _, name := range up.Models {
			if seen[name] {
				continue
			}
			seen[name] = true
			list = append(list, model{ID: name, Object: "model", OwnedBy: up.Name})
		}
	}
	writeJSON(w, http.StatusOK, map[string]any{"object": "list", "data": list})
}

// handleProxy 处理 /v1/ 下的所有请求：解析 model → 选上游 → 透传（含 SSE 流）。
func handleProxy(w http.ResponseWriter, r *http.Request) {
	body, err := io.ReadAll(io.LimitReader(r.Body, maxBodyBytes))
	if err != nil {
		writeOpenAIError(w, http.StatusBadRequest, "读取请求体失败")
		return
	}
	var probe struct {
		Model string `json:"model"`
	}
	_ = json.Unmarshal(body, &probe)

	up, err := pickUpstream(probe.Model)
	if err != nil {
		writeOpenAIError(w, http.StatusNotFound, err.Error())
		return
	}

	target := strings.TrimRight(up.BaseURL, "/") + r.URL.Path
	if r.URL.RawQuery != "" {
		target += "?" + r.URL.RawQuery
	}
	req, err := http.NewRequestWithContext(r.Context(), r.Method, target, strings.NewReader(string(body)))
	if err != nil {
		writeOpenAIError(w, http.StatusInternalServerError, "构造上游请求失败")
		return
	}
	req.Header.Set("Authorization", "Bearer "+up.APIKey)
	if ct := r.Header.Get("Content-Type"); ct != "" {
		req.Header.Set("Content-Type", ct)
	} else {
		req.Header.Set("Content-Type", "application/json")
	}
	if accept := r.Header.Get("Accept"); accept != "" {
		req.Header.Set("Accept", accept)
	}

	start := time.Now()
	resp, err := relayClient.Do(req)
	if err != nil {
		log.Printf("felix-llm-relay: %s %s → %s 失败：%v", r.Method, r.URL.Path, up.Name, err)
		writeOpenAIError(w, http.StatusBadGateway, "上游请求失败："+err.Error())
		return
	}
	defer resp.Body.Close()

	if ct := resp.Header.Get("Content-Type"); ct != "" {
		w.Header().Set("Content-Type", ct)
	}
	w.WriteHeader(resp.StatusCode)
	flusher, _ := w.(http.Flusher)
	written, copyErr := io.Copy(&flushWriter{w: w, f: flusher}, resp.Body)
	if copyErr != nil {
		// 流式过程中客户端断开是常态，不打错误日志
		if !errors.Is(copyErr, context.Canceled) {
			log.Printf("felix-llm-relay: 转发中断（%s → %s）：%v", up.Name, probe.Model, copyErr)
		}
	}
	log.Printf("felix-llm-relay: %s %s model=%q upstream=%s status=%d bytes=%d 用时=%s",
		r.Method, r.URL.Path, probe.Model, up.Name, resp.StatusCode, written, time.Since(start).Round(time.Millisecond))
}

func pickUpstream(model string) (Upstream, error) {
	ups := currentConfig().Upstreams
	if len(ups) == 0 {
		return Upstream{}, errors.New("尚未配置任何上游：请编辑 " + cfgPath)
	}
	pick := func(up Upstream) (Upstream, error) {
		// 远端上游必须带 Key（本地/回环的无鉴权服务如 Ollama 允许为空）
		if up.APIKey == "" && !isLoopbackBase(up.BaseURL) {
			return Upstream{}, fmt.Errorf("上游 %q 未配置 apiKey（编辑 %s 填入后自动生效）", up.Name, cfgPath)
		}
		return up, nil
	}
	for _, up := range ups {
		for _, m := range up.Models {
			if m == model {
				return pick(up)
			}
		}
	}
	for _, up := range ups {
		if up.Default {
			return pick(up)
		}
	}
	if len(ups) == 1 && model == "" {
		return pick(ups[0])
	}
	if model == "" {
		return Upstream{}, errors.New("请求缺少 model 且没有默认上游")
	}
	return Upstream{}, fmt.Errorf("模型 %q 未映射到任何上游（在 %s 的 models 里声明，或给某上游加 \"default\": true）", model, cfgPath)
}

// isLoopbackBase 判断上游是否为本机回环（无鉴权的本地推理服务通常不需要 Key）。
func isLoopbackBase(baseURL string) bool {
	u := baseURL
	if i := strings.Index(u, "://"); i >= 0 {
		u = u[i+3:]
	}
	if i := strings.IndexAny(u, "/?#"); i >= 0 {
		u = u[:i]
	}
	host := u
	if i := strings.LastIndex(u, ":"); i >= 0 {
		host = u[:i]
	}
	return host == "127.0.0.1" || host == "localhost" || host == "::1" || host == "[::1]"
}

// flushWriter 让 SSE 每写一块就刷给客户端，避免被缓冲拖住。
type flushWriter struct {
	w io.Writer
	f http.Flusher
}

func (fw *flushWriter) Write(p []byte) (int, error) {
	n, err := fw.w.Write(p)
	if fw.f != nil {
		fw.f.Flush()
	}
	return n, err
}

func writeJSON(w http.ResponseWriter, code int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(code)
	_ = json.NewEncoder(w).Encode(v)
}

func writeOpenAIError(w http.ResponseWriter, code int, msg string) {
	writeJSON(w, code, map[string]any{
		"error": map[string]any{"message": msg, "type": "felix_relay_error"},
	})
}

var relayClient = &http.Client{
	Transport: &http.Transport{
		ResponseHeaderTimeout: responseHdrTimeout,
		IdleConnTimeout:       90 * time.Second,
		MaxIdleConnsPerHost:   8,
	},
	Timeout: upstreamTimeout,
}
