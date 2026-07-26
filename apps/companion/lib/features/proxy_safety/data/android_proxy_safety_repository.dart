import 'package:flutter/services.dart';

import 'proxy_safety_repository.dart';

class AndroidProxySafetyRepository implements ProxySafetyRepository {
  static const _channel = MethodChannel('dev.prayag.apptester/proxy_safety');

  @override
  Future<ProxySafetyStatus> status() => _invoke('status');

  @override
  Future<ProxySafetyStatus> arm({required String host, required int port}) =>
      _invoke('arm', {'host': host, 'port': port});

  @override
  Future<ProxySafetyStatus> disarm() => _invoke('disarm');

  Future<ProxySafetyStatus> _invoke(String method, [Object? arguments]) async {
    final result =
        await _channel.invokeMethod<Map<Object?, Object?>>(method, arguments);
    if (result == null) {
      throw StateError('Android companion returned no status.');
    }
    return ProxySafetyStatus.fromMap(result);
  }
}
