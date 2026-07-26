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

  Future<void> startMonitoring(String host, String portText) async {
    final port = int.tryParse(portText);
    if (host.trim().isEmpty || port == null || port < 1 || port > 65535) {
      error = 'Enter a reachable desktop host and a port from 1 to 65535.';
      notifyListeners();
      return;
    }
    await _run(
        () => _repository.startMonitoring(host: host.trim(), port: port));
    _syncRefreshTimer();
  }

  Future<void> stopMonitoring() async {
    await _run(_repository.stopMonitoring);
    _syncRefreshTimer();
  }

  Future<void> startVpn(String host, String portText, String targetPackage) async {
    final port = int.tryParse(portText);
    if (host.trim().isEmpty || port == null || port < 1 || port > 65535 || targetPackage.trim().isEmpty) {
      error = 'Enter a desktop host, a port from 1 to 65535, and the selected package.';
      notifyListeners();
      return;
    }
    await _run(() => _repository.startVpn(host: host.trim(), port: port, targetPackage: targetPackage.trim()));
    _syncRefreshTimer();
  }

  Future<void> stopVpn() async {
    await _run(_repository.stopVpn);
    _syncRefreshTimer();
  }

  void _syncRefreshTimer() {
    _refreshTimer?.cancel();
    if (status?.isMonitoring != true && status?.isVpnActive != true) return;
    _refreshTimer = Timer.periodic(
      const Duration(seconds: 5),
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
