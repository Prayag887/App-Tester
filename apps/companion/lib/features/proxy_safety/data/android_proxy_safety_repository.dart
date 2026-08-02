import 'package:flutter/services.dart';

import 'proxy_safety_repository.dart';

class AndroidProxySafetyRepository implements ProxySafetyRepository {
  static const _channel = MethodChannel('dev.prayag.apptester/proxy_safety');

  @override
  Future<ProxySafetyStatus> status() => _invoke('status');

  @override
  Future<ProxySafetyStatus> startVpn() => _invoke('startVpn');

  @override
  Future<ProxySafetyStatus> stopVpn() => _invoke('stopVpn');

  @override
  Future<ProxySafetyStatus> installCa() => _invoke('installCa');

  @override
  Future<ProxySafetyStatus> removeCa() => _invoke('removeCa');

  Future<ProxySafetyStatus> _invoke(String method, [Object? arguments]) async {
    final result =
        await _channel.invokeMethod<Map<Object?, Object?>>(method, arguments);
    if (result == null) {
      throw StateError('Android companion returned no status.');
    }
    return ProxySafetyStatus.fromMap(result);
  }
}
