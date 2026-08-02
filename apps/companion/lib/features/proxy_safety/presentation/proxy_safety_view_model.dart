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
    _refreshTimer?.cancel();
    _refreshTimer = Timer.periodic(
      const Duration(seconds: 1),
      (_) => _run(_repository.status),
    );
  }

  Future<void> disconnect() => _run(_repository.stopVpn);

  Future<void> installCa() => _run(_repository.installCa);

  Future<void> removeCa() => _run(_repository.removeCa);

  @override
  void dispose() {
    _refreshTimer?.cancel();
    super.dispose();
  }

  Future<void> _run(Future<ProxySafetyStatus> Function() action) async {
    if (isWorking) return;
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
