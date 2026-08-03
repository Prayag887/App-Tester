package main

/*
#include <jni.h>
#include <stdlib.h>
#include <string.h>

static char* appTesterString(JNIEnv* env, jstring value) {
  const char* raw = (*env)->GetStringUTFChars(env, value, 0);
  if (raw == NULL) return NULL;
  char* copy = strdup(raw);
  (*env)->ReleaseStringUTFChars(env, value, raw);
  return copy;
}

static jstring appTesterJString(JNIEnv* env, const char* value) {
  return (*env)->NewStringUTF(env, value);
}
*/
import "C"

import (
	"fmt"
	"sync"
	"syscall"
	"unsafe"

	_ "github.com/xjasonlyu/tun2socks/v2/dns"
	"github.com/xjasonlyu/tun2socks/v2/engine"
)

var (
	engineLock  sync.Mutex
	activeTunFD = -1
)

// stopTunnel releases both layers that own the detached Android TUN file
// descriptor. VpnService cannot close a descriptor after detachFd(), so the
// native relay must do it or Android will continue to show an active VPN.
func stopTunnel() {
	engine.Stop()
	if activeTunFD >= 0 {
		_ = syscall.Close(activeTunFD)
		activeTunFD = -1
	}
}

//export Java_dev_prayag_apptester_companion_VpnNative_start
func Java_dev_prayag_apptester_companion_VpnNative_start(env *C.JNIEnv, clazz C.jclass, fd C.jint, proxy C.jstring) C.jstring {
	engineLock.Lock()
	defer engineLock.Unlock()

	value := C.appTesterString(env, proxy)
	if value == nil {
		return C.appTesterJString(env, C.CString("Unable to read the desktop proxy address."))
	}
	defer C.free(unsafe.Pointer(value))

	stopTunnel()
	engine.Insert(&engine.Key{
		Device:   fmt.Sprintf("fd://%d", int(fd)),
		Proxy:    C.GoString(value),
		MTU:      1500,
		LogLevel: "warn",
	})
	// engine.Start configures the gVisor packet stack and returns immediately.
	// The configured device and stack remain active until stop is called.
	engine.Start()
	activeTunFD = int(fd)
	return C.appTesterJString(env, C.CString(""))
}

//export Java_dev_prayag_apptester_companion_VpnNative_stop
func Java_dev_prayag_apptester_companion_VpnNative_stop(env *C.JNIEnv, clazz C.jclass) {
	engineLock.Lock()
	defer engineLock.Unlock()
	stopTunnel()
}

func main() {}
