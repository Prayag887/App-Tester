import 'package:flutter/foundation.dart';

import '../data/proxy_safety_repository.dart';

class ProxySafetyViewModel extends ChangeNotifier {
  ProxySafetyViewModel(this._repository);

  final ProxySafetyRepository _repository;
  ProxySafetyStatus? status;
  bool isWorking = false;
  String? error;

  Future<void> load() => _run(_repository.status);

  Future<void> arm(String host, String portText) async {
    final port = int.tryParse(portText);
    if (host.trim().isEmpty || port == null || port < 1 || port > 65535) {
      error = 'Enter a reachable desktop host and a port from 1 to 65535.';
      notifyListeners();
      return;
    }
    await _run(() => _repository.arm(host: host.trim(), port: port));
  }

  Future<void> disarm() => _run(_repository.disarm);

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
