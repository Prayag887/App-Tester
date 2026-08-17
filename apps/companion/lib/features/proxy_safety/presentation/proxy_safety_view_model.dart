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

  Future<void> stopVpn() async {
    await _run(_repository.stopVpn);
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
