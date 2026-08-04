import 'dart:async';

import 'package:flutter/foundation.dart';

import '../data/proxy_safety_repository.dart';

class ProxySafetyViewModel extends ChangeNotifier {
  ProxySafetyViewModel(this._repository);

  final ProxySafetyRepository _repository;
  ProxySafetyStatus? status;
  bool isWorking = false;
  String? error;
  Timer? _refreshTimer;

  Future<void> load() async {
    await _run(_repository.status);
    _syncRefreshTimer();
  }

  /// Connection choices belong to the person using the companion. The desktop
  /// only opens this screen; it never starts or stops the phone's VPN.
  Future<void> connectVpn({
    required String host,
    required String portText,
    required String targetPackage,
  }) async {
    final port = int.tryParse(portText);
    if (host.trim().isEmpty ||
        port == null ||
        port < 1 ||
        port > 65535 ||
        targetPackage.trim().isEmpty) {
      error =
          'Enter a desktop host, a port from 1 to 65535, and a package name.';
      notifyListeners();
      return;
    }
    await _run(
        () => _repository.startMonitoring(host: host.trim(), port: port));
    if (error == null) {
      await _run(() => _repository.startVpn(
            host: host.trim(),
            port: port,
            targetPackage: targetPackage.trim(),
          ));
    }
    _syncRefreshTimer();
  }

  Future<void> stopVpn() async {
    await _run(_repository.stopVpn);
    _syncRefreshTimer();
  }

  Future<void> disconnect() async {
    if (status?.isVpnActive == true) await _run(_repository.stopVpn);
    await _run(_repository.stopMonitoring);
    _syncRefreshTimer();
  }

  void _syncRefreshTimer() {
    _refreshTimer?.cancel();
    if (status?.isMonitoring != true && status?.isVpnActive != true) return;
    _refreshTimer = Timer.periodic(
      const Duration(seconds: 3),
      (_) => _run(_repository.status),
    );
  }

  @override
  void dispose() {
    _refreshTimer?.cancel();
    super.dispose();
  }

  Future<void> _run(Future<ProxySafetyStatus> Function() action) async {
    isWorking = true;
    error = null;
    notifyListeners();
    try {
      status = await action();
    } catch (exception) {
      error = exception.toString();
    } finally {
      isWorking = false;
      notifyListeners();
    }
  }
}
